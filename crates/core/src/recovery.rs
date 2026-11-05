//! 異常終了からの復旧用の写し。元のファイルとは別の場所に置く。
//!
//! 形式（UTF-8以外の元のパスも保てるよう、バイト列として扱う）:
//!
//! ```text
//! kakikotoba-recovery 1
//! pid <プロセスID>
//! saved <書き出した時刻。UNIX時刻のミリ秒>
//! path <元のパス。新規なら空。`\` と改行はエスケープする>
//!
//! <本文>
//! ```

use std::ffi::OsString;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ropey::Rope;

const MAGIC: &[u8] = b"kakikotoba-recovery 1";
const EXTENSION: &str = "kakikotoba";

/// 写しを置くディレクトリ。`$XDG_STATE_HOME`（未設定なら `~/.local/state`）の下。
pub fn default_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/state")))?;
    Some(state.join("kakikotoba/recovery"))
}

/// この起動で書き出す写し。
pub struct Session {
    file: PathBuf,
    /// 最後に書き出した本文の `Buffer::revision`。写しがなければ `None`
    written: Option<u64>,
}

impl Session {
    pub fn new(dir: &Path) -> Self {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let name = format!("{started}-{}.{EXTENSION}", std::process::id());
        Self { file: dir.join(name), written: None }
    }

    /// 前回から本文が変わっていれば書き出す。一時ファイルに書いてから置き換える。
    pub fn write(&mut self, path: Option<&Path>, text: &Rope, revision: u64) -> io::Result<()> {
        if self.written == Some(revision) {
            return Ok(());
        }
        let dir = self.file.parent().expect("ディレクトリの下に作る");
        // 原稿の写しなので、本人だけが読めるようにする
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let mut tmp_name = self.file.file_name().expect("名前は自分で付けた").to_os_string();
        tmp_name.push(".tmp");
        let tmp = self.file.with_file_name(tmp_name);
        let result = write_snapshot(&tmp, path, text).and_then(|()| fs::rename(&tmp, &self.file));
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        self.written = Some(revision);
        Ok(())
    }

    /// 写しを消す。もともとなければ何もしない。
    pub fn remove(&mut self) -> io::Result<()> {
        if self.written.take().is_some() {
            match fs::remove_file(&self.file) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}

fn write_snapshot(tmp: &Path, path: Option<&Path>, text: &Rope) -> io::Result<()> {
    let file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(tmp)?;
    let mut writer = BufWriter::new(file);
    let saved = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    writer.write_all(MAGIC)?;
    write!(writer, "\npid {}\nsaved {saved}\npath ", std::process::id())?;
    writer.write_all(&escape(path.map_or(&[][..], |path| path.as_os_str().as_bytes())))?;
    writer.write_all(b"\n\n")?;
    text.write_to(&mut writer)?;
    writer.into_inner().map_err(|e| e.into_error())?.sync_all()
}

fn escape(bytes: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'\\' => escaped.extend_from_slice(b"\\\\"),
            b'\n' => escaped.extend_from_slice(b"\\n"),
            _ => escaped.push(b),
        }
    }
    escaped
}

fn unescape(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut plain = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter();
    while let Some(&b) = iter.next() {
        plain.push(match b {
            b'\\' => match iter.next()? {
                b'\\' => b'\\',
                b'n' => b'\n',
                _ => return None,
            },
            _ => b,
        });
    }
    Some(plain)
}

/// 異常終了した起動が残した写し。
#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub file: PathBuf,
    pid: u32,
    /// 元のパス。新規文書なら `None`
    pub path: Option<PathBuf>,
    pub saved: SystemTime,
    pub text: String,
}

impl Snapshot {
    fn parse(file: PathBuf, data: &[u8]) -> Option<Self> {
        let mut rest = data.strip_prefix(MAGIC)?.strip_prefix(b"\n")?;
        let mut field = |name: &[u8]| {
            let end = rest.iter().position(|&b| b == b'\n')?;
            let value = rest[..end].strip_prefix(name)?.strip_prefix(b" ")?;
            rest = &rest[end + 1..];
            Some(value)
        };
        let number = |value: &[u8]| std::str::from_utf8(value).ok()?.parse::<u64>().ok();
        let pid = u32::try_from(number(field(b"pid")?)?).ok()?;
        let saved = UNIX_EPOCH + Duration::from_millis(number(field(b"saved")?)?);
        let path = unescape(field(b"path")?)?;
        let path = (!path.is_empty()).then(|| PathBuf::from(OsString::from_vec(path)));
        let text = String::from_utf8(rest.strip_prefix(b"\n")?.to_vec()).ok()?;
        Some(Self { file, pid, path, saved, text })
    }

    /// 元のファイルが写しより後に更新されているか。
    pub fn original_is_newer(&self) -> bool {
        let modified = self.path.as_deref().and_then(|path| fs::metadata(path).ok()).and_then(|meta| meta.modified().ok());
        modified.is_some_and(|modified| modified > self.saved)
    }

    pub fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// 持ち主の起動がもう動いていない写しを、新しい順に返す。読めない写しは飛ばす。
pub fn orphans(dir: &Path) -> Vec<Snapshot> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut snapshots: Vec<Snapshot> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|file| file.extension().is_some_and(|ext| ext == EXTENSION))
        .filter_map(|file| {
            let data = fs::read(&file).ok()?;
            Snapshot::parse(file, &data)
        })
        .filter(|snapshot| !is_running(snapshot.pid))
        .collect();
    snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.saved));
    snapshots
}

/// そのプロセスIDで、このアプリが動いているか。IDの使い回しに備えて、実行ファイルの名前も比べる。
fn is_running(pid: u32) -> bool {
    let comm = |pid: &str| fs::read(format!("/proc/{pid}/comm")).ok();
    comm(&pid.to_string()).is_some_and(|theirs| Some(theirs) == comm("self"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kakikotoba-recovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// 動いていない起動が残したことにする。
    fn orphan_of(session: &Session) {
        let data = fs::read(&session.file).unwrap();
        let pid = format!("pid {}\n", std::process::id());
        let start = data.windows(pid.len()).position(|w| w == pid.as_bytes()).unwrap();
        let mut patched = data[..start].to_vec();
        // pid 0 はカーネルのもので、/proc/0 はない
        patched.extend_from_slice(b"pid 0\n");
        patched.extend_from_slice(&data[start + pid.len()..]);
        fs::write(&session.file, patched).unwrap();
    }

    #[test]
    fn written_snapshot_is_listed_only_after_its_owner_stops() {
        let dir = temp_dir("orphan");
        let mut session = Session::new(&dir);
        let path = Path::new("/原稿/改行\nと\\を含む.txt");
        session.write(Some(path), &Rope::from_str("吾輩は猫である。\n"), 1).unwrap();
        // 動いている起動の写しは出さない
        assert!(orphans(&dir).is_empty());

        orphan_of(&session);
        let found = orphans(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path.as_deref(), Some(path));
        assert_eq!(found[0].text, "吾輩は猫である。\n");
        assert!(!found[0].original_is_newer());
        found[0].remove().unwrap();
        assert!(orphans(&dir).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn session_writes_only_on_change_and_removes_its_file() {
        let dir = temp_dir("session");
        let mut session = Session::new(&dir);
        session.write(None, &Rope::from_str("あ"), 1).unwrap();
        fs::remove_file(&session.file).unwrap();
        // 同じ版なら書き直さない
        session.write(None, &Rope::from_str("あ"), 1).unwrap();
        assert!(!session.file.exists());
        session.write(None, &Rope::from_str("い"), 2).unwrap();
        assert!(session.file.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "一時ファイルが残っている");

        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&session.file).unwrap().permissions().mode() & 0o777, 0o600);

        session.remove().unwrap();
        assert!(!session.file.exists());
        // 消した後は、同じ版でも書き直す
        session.write(None, &Rope::from_str("い"), 2).unwrap();
        assert!(session.file.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn newest_first_and_broken_files_are_skipped() {
        let dir = temp_dir("order");
        fs::create_dir_all(&dir).unwrap();
        let snapshot = |name: &str, saved: u64, text: &str| {
            let data = format!("kakikotoba-recovery 1\npid 0\nsaved {saved}\npath \n\n{text}");
            fs::write(dir.join(name), data).unwrap();
        };
        snapshot("1-1.kakikotoba", 1000, "古い");
        snapshot("2-1.kakikotoba", 2000, "新しい");
        fs::write(dir.join("3-1.kakikotoba"), "壊れた写し").unwrap();
        fs::write(dir.join("4-1.kakikotoba.tmp"), "書きかけ").unwrap();
        let texts: Vec<String> = orphans(&dir).into_iter().map(|s| s.text).collect();
        assert_eq!(texts, ["新しい", "古い"]);
        assert_eq!(orphans(&dir)[0].path, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn original_updated_after_the_snapshot_is_reported() {
        let dir = temp_dir("newer");
        fs::create_dir_all(&dir).unwrap();
        let original = dir.join("本文.txt");
        fs::write(&original, "保存済み").unwrap();
        let snapshot = Snapshot {
            file: dir.join("x.kakikotoba"),
            pid: 0,
            path: Some(original),
            saved: UNIX_EPOCH,
            text: String::new(),
        };
        assert!(snapshot.original_is_newer());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn state_dir_follows_xdg_and_falls_back_to_home() {
        // 環境変数はプロセス全体で共有されるので、1つのテストの中で順に確かめる
        let saved = (std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"));
        // SAFETY: このクレートのテストで環境変数を読むのはこのテストだけ
        unsafe {
            std::env::set_var("XDG_STATE_HOME", "/state");
            assert_eq!(default_dir(), Some(PathBuf::from("/state/kakikotoba/recovery")));
            // 相対パスは仕様で無視する
            std::env::set_var("XDG_STATE_HOME", "relative");
            std::env::set_var("HOME", "/home/me");
            assert_eq!(default_dir(), Some(PathBuf::from("/home/me/.local/state/kakikotoba/recovery")));
            std::env::remove_var("XDG_STATE_HOME");
            assert_eq!(default_dir(), Some(PathBuf::from("/home/me/.local/state/kakikotoba/recovery")));
            for (name, value) in [("XDG_STATE_HOME", saved.0), ("HOME", saved.1)] {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}
