//! fcitx5へのDBus接続。Qtに依存しない。
//!
//! 受信は1つのスレッドにまとめ、シグナルと `ProcessKeyEvent` の応答を届いた順に `Event` で渡す。
//! 順序が保たれるので、確定の直後に来る未処理キーが確定より先に処理されることはない。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use zbus::blocking::{Connection, MessageIterator};
use zbus::message::{Flags, Message, Type};
use zbus::zvariant::{DynamicType, OwnedObjectPath};

use crate::editor::CandidatePanel;

const SERVICE: &str = "org.fcitx.Fcitx5";
const INPUT_METHOD_PATH: &str = "/org/freedesktop/portal/inputmethod";
const INPUT_METHOD_INTERFACE: &str = "org.fcitx.Fcitx.InputMethod1";
const INPUT_CONTEXT_INTERFACE: &str = "org.fcitx.Fcitx.InputContext1";
const OWNER_MATCH_RULE: &str =
    "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.fcitx.Fcitx5'";

// fcitx-utils/capabilityflags.h。ClientUnfocusCommitを立てると、FocusOutでfcitx5が確定しなくなる
const CAPABILITY_PREEDIT: u64 = 1 << 1;
const CAPABILITY_FORMATTED_PREEDIT: u64 = 1 << 4;
const CAPABILITY_KEY_EVENT_ORDER_FIX: u64 = 1 << 37;
const CAPABILITY_CLIENT_SIDE_INPUT_PANEL: u64 = 1 << 39;
const CAPABILITIES: u64 = CAPABILITY_PREEDIT
    | CAPABILITY_FORMATTED_PREEDIT
    | CAPABILITY_KEY_EVENT_ORDER_FIX
    | CAPABILITY_CLIENT_SIDE_INPUT_PANEL;

const KEYSYM_BACKSPACE: u32 = 0xff08;
const KEYSYM_RETURN: u32 = 0xff0d;
const KEYSYM_KP_ENTER: u32 = 0xff8d;
const KEYSYM_DELETE: u32 = 0xffff;
const KEYSYM_LEFT: u32 = 0xff51;
const KEYSYM_UP: u32 = 0xff52;
const KEYSYM_RIGHT: u32 = 0xff53;
const KEYSYM_DOWN: u32 = 0xff54;
/// evdevのキーコードに8を足した値
const KEYCODE_UP: u32 = 111;
const KEYCODE_LEFT: u32 = 113;
const KEYCODE_RIGHT: u32 = 114;
const KEYCODE_DOWN: u32 = 116;
const STATE_SHIFT: u32 = 1;

/// Qtのキーイベントのうち、fcitx5が処理しなかったときに要るもの。
pub struct QtKey {
    pub key: i32,
    pub modifiers: i32,
    pub text: String,
}

/// fcitx5へ送るキー。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeKey {
    pub sym: u32,
    pub code: u32,
    pub state: u32,
    pub time: u32,
}

pub enum Event {
    Connected(Client),
    Disconnected,
    /// 文節ごとの文字列と書式、UTF-8のバイト単位のカーソル位置（負ならなし）
    Preedit(Vec<(String, i32)>, i32),
    Panel(CandidatePanel),
    Commit(String),
    /// fcitx5が処理しなかったキー
    Unhandled(QtKey),
}

/// 変換中の矢印キーを読み替える。文字列は縦、候補は横に並ぶので、方向をエンジンの想定に合わせる。
pub fn remap_composing_key(key: NativeKey) -> NativeKey {
    let (sym, code) = match key.sym {
        KEYSYM_UP => (KEYSYM_LEFT, KEYCODE_LEFT),
        KEYSYM_DOWN => (KEYSYM_RIGHT, KEYCODE_RIGHT),
        KEYSYM_LEFT => (KEYSYM_DOWN, KEYCODE_DOWN),
        KEYSYM_RIGHT => (KEYSYM_UP, KEYCODE_UP),
        _ => return key,
    };
    NativeKey { sym, code, ..key }
}

/// `ForwardKey` のキーシムをQtのキーへ戻す。編集に使わないキーは `None`。
fn qt_key_from_keysym(sym: u32, state: u32) -> Option<QtKey> {
    const KEY_BACKSPACE: i32 = 0x0100_0003;
    const KEY_RETURN: i32 = 0x0100_0004;
    const KEY_DELETE: i32 = 0x0100_0007;
    const KEY_LEFT: i32 = 0x0100_0012;
    const KEY_UP: i32 = 0x0100_0013;
    const KEY_RIGHT: i32 = 0x0100_0014;
    const KEY_DOWN: i32 = 0x0100_0015;
    const SHIFT: i32 = 0x0200_0000;
    const UNICODE_KEYSYM_BASE: u32 = 0x0100_0000;

    let modifiers = if state & STATE_SHIFT != 0 { SHIFT } else { 0 };
    let named = |key| Some(QtKey { key, modifiers, text: String::new() });
    match sym {
        KEYSYM_BACKSPACE => named(KEY_BACKSPACE),
        KEYSYM_RETURN | KEYSYM_KP_ENTER => named(KEY_RETURN),
        KEYSYM_DELETE => named(KEY_DELETE),
        KEYSYM_LEFT => named(KEY_LEFT),
        KEYSYM_UP => named(KEY_UP),
        KEYSYM_RIGHT => named(KEY_RIGHT),
        KEYSYM_DOWN => named(KEY_DOWN),
        _ => {
            let code = if sym > UNICODE_KEYSYM_BASE { sym - UNICODE_KEYSYM_BASE } else { sym };
            let printable = sym > UNICODE_KEYSYM_BASE || (0x20..0x7f).contains(&sym);
            let c = char::from_u32(code).filter(|_| printable)?;
            Some(QtKey { key: c.to_ascii_uppercase() as i32, modifiers, text: c.to_string() })
        }
    }
}

/// 候補ラベル（`1. `）を候補の前に付ける形へ整える。
fn panel_from_ui(
    aux_up: Vec<(String, i32)>,
    aux_down: Vec<(String, i32)>,
    candidates: Vec<(String, String)>,
    selected: i32,
) -> CandidatePanel {
    let aux = aux_up.iter().chain(&aux_down).map(|(text, _)| text.as_str()).collect::<String>();
    CandidatePanel {
        candidates: candidates
            .into_iter()
            .map(|(label, text)| {
                let label = label.trim_end_matches([' ', '.']);
                let separator = if label.is_empty() { "" } else { "　" };
                format!("{label}{separator}{}", text.trim_end())
            })
            .collect(),
        selected: usize::try_from(selected).ok(),
        aux,
    }
}

/// 入力コンテキストへの送信口。送信はUIスレッドから行う。
#[derive(Clone)]
pub struct Client {
    connection: Connection,
    path: OwnedObjectPath,
    pending_keys: Arc<Mutex<HashMap<u32, QtKey>>>,
}

impl Client {
    fn message<B>(&self, method: &str, reply: bool, body: &B) -> zbus::Result<Message>
    where
        B: serde::Serialize + DynamicType,
    {
        let mut builder = Message::method_call(self.path.as_str(), method)?
            .destination(SERVICE)?
            .interface(INPUT_CONTEXT_INTERFACE)?;
        if !reply {
            builder = builder.with_flags(Flags::NoReplyExpected)?;
        }
        builder.build(body)
    }

    /// 応答を待たずに送る。接続が切れていれば `NameOwnerChanged` で分かるので、失敗は無視する。
    fn notify<B>(&self, method: &str, body: &B)
    where
        B: serde::Serialize + DynamicType,
    {
        if let Ok(message) = self.message(method, false, body) {
            let _ = self.connection.send(&message);
        }
    }

    /// キーの押下を送る。fcitx5が処理しなければ `Event::Unhandled` で戻ってくる。
    /// 送れなければ `key` を返す。
    pub fn press(&self, native: NativeKey, key: QtKey) -> Result<(), QtKey> {
        let body = (native.sym, native.code, native.state, false, native.time);
        let Ok(message) = self.message("ProcessKeyEvent", true, &body) else {
            return Err(key);
        };
        let serial = u32::from(message.primary_header().serial_num());
        // 応答が先に届いても取りこぼさないよう、送る前に登録する
        self.pending_keys.lock().expect("保持中にpanicしない").insert(serial, key);
        if self.connection.send(&message).is_err() {
            let key = self.pending_keys.lock().expect("保持中にpanicしない").remove(&serial);
            return Err(key.expect("登録したばかり"));
        }
        Ok(())
    }

    pub fn release(&self, native: NativeKey) {
        self.notify("ProcessKeyEvent", &(native.sym, native.code, native.state, true, native.time));
    }

    pub fn focus_in(&self) {
        self.notify("FocusIn", &());
    }

    /// 未確定の文字があれば、fcitx5が確定する。
    pub fn focus_out(&self) {
        self.notify("FocusOut", &());
    }

    /// 未確定の文字を破棄する。
    pub fn reset(&self) {
        self.notify("Reset", &());
    }

    pub fn select_candidate(&self, index: usize) {
        self.notify("SelectCandidate", &(index as i32));
    }
}

fn create_client(connection: &Connection, pending_keys: &Arc<Mutex<HashMap<u32, QtKey>>>) -> zbus::Result<Client> {
    let args = [("program", "kakikotoba"), ("display", "kakikotoba")];
    let reply = connection.call_method(
        Some(SERVICE),
        INPUT_METHOD_PATH,
        Some(INPUT_METHOD_INTERFACE),
        "CreateInputContext",
        &(args.as_slice(),),
    )?;
    let (path, _uuid): (OwnedObjectPath, Vec<u8>) = reply.body().deserialize()?;
    let client = Client { connection: connection.clone(), path, pending_keys: pending_keys.clone() };
    client.notify("SetCapability", &CAPABILITIES);
    Ok(client)
}

/// 受信スレッドの状態。
struct Receiver<F> {
    connection: Connection,
    pending_keys: Arc<Mutex<HashMap<u32, QtKey>>>,
    path: Option<OwnedObjectPath>,
    on_event: F,
}

impl<F: FnMut(Event)> Receiver<F> {
    fn connect(&mut self) {
        // fcitx5がいない環境ではここで失敗する。起動すれば `NameOwnerChanged` で再び呼ばれる
        if let Ok(client) = create_client(&self.connection, &self.pending_keys) {
            self.path = Some(client.path.clone());
            (self.on_event)(Event::Connected(client));
        }
    }

    fn disconnect(&mut self) {
        self.path = None;
        (self.on_event)(Event::Disconnected);
        let keys: Vec<QtKey> = self.pending_keys.lock().expect("保持中にpanicしない").drain().map(|(_, key)| key).collect();
        for key in keys {
            (self.on_event)(Event::Unhandled(key));
        }
    }

    fn handle_reply(&mut self, message: &Message, is_error: bool) {
        let Some(serial) = message.header().reply_serial() else {
            return;
        };
        let key = self.pending_keys.lock().expect("保持中にpanicしない").remove(&u32::from(serial));
        if let Some(key) = key {
            let handled = !is_error && message.body().deserialize::<bool>().unwrap_or(false);
            if !handled {
                (self.on_event)(Event::Unhandled(key));
            }
        }
    }

    fn handle_signal(&mut self, message: &Message) -> zbus::Result<()> {
        let header = message.header();
        let member = header.member().map_or("", |m| m.as_str());
        let body = message.body();
        if member == "NameOwnerChanged" {
            let (name, _old, new): (String, String, String) = body.deserialize()?;
            if name == SERVICE {
                self.disconnect();
                if !new.is_empty() {
                    self.connect();
                }
            }
            return Ok(());
        }
        if header.path().map(|p| p.as_str()) != self.path.as_ref().map(|p| p.as_str()) {
            return Ok(());
        }
        match member {
            "UpdateFormattedPreedit" => {
                let (segments, cursor): (Vec<(String, i32)>, i32) = body.deserialize()?;
                (self.on_event)(Event::Preedit(segments, cursor));
            }
            "UpdateClientSideUI" => {
                type Text = Vec<(String, i32)>;
                type Ui = (Text, i32, Text, Text, Vec<(String, String)>, i32, i32, bool, bool);
                let (_, _, aux_up, aux_down, candidates, selected, ..): Ui = body.deserialize()?;
                (self.on_event)(Event::Panel(panel_from_ui(aux_up, aux_down, candidates, selected)));
            }
            "CommitString" => (self.on_event)(Event::Commit(body.deserialize()?)),
            "ForwardKey" => {
                let (sym, state, is_release): (u32, u32, bool) = body.deserialize()?;
                if let Some(key) = qt_key_from_keysym(sym, state).filter(|_| !is_release) {
                    (self.on_event)(Event::Unhandled(key));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn run(&mut self, messages: MessageIterator) {
        self.connect();
        for message in messages {
            let Ok(message) = message else {
                continue;
            };
            match message.message_type() {
                Type::MethodReturn => self.handle_reply(&message, false),
                Type::Error => self.handle_reply(&message, true),
                Type::Signal => {
                    let _ = self.handle_signal(&message);
                }
                Type::MethodCall => {}
            }
        }
    }
}

/// 受信スレッドを起動する。`on_event` はこのスレッドで呼ばれる。
/// セッションバスに繋がらなければ何も起きず、QtのIM経路のままになる。
pub fn spawn(on_event: impl FnMut(Event) + Send + 'static) {
    thread::spawn(move || {
        let Ok(connection) = Connection::session() else {
            return;
        };
        let messages = MessageIterator::from(&connection);
        let add_match = connection.call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "AddMatch",
            &OWNER_MATCH_RULE,
        );
        if add_match.is_err() {
            return;
        }
        let pending_keys = Arc::default();
        Receiver { connection, pending_keys, path: None, on_event }.run(messages);
    });
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    fn native(sym: u32) -> NativeKey {
        NativeKey { sym, code: 1, state: STATE_SHIFT, time: 2 }
    }

    #[test]
    fn composing_arrows_follow_the_direction_of_preedit_and_candidates() {
        let expected = [
            (KEYSYM_UP, KEYSYM_LEFT, KEYCODE_LEFT),
            (KEYSYM_DOWN, KEYSYM_RIGHT, KEYCODE_RIGHT),
            (KEYSYM_LEFT, KEYSYM_DOWN, KEYCODE_DOWN),
            (KEYSYM_RIGHT, KEYSYM_UP, KEYCODE_UP),
        ];
        for (from, sym, code) in expected {
            assert_eq!(remap_composing_key(native(from)), NativeKey { sym, code, state: STATE_SHIFT, time: 2 });
        }
        assert_eq!(remap_composing_key(native(0x61)), native(0x61));
    }

    #[test]
    fn forwarded_keysyms_become_qt_keys() {
        assert_eq!(qt_key_from_keysym(0x61, 0).unwrap().text, "a");
        assert_eq!(qt_key_from_keysym(0x0100_3042, 0).unwrap().text, "あ");
        let key = qt_key_from_keysym(KEYSYM_DOWN, STATE_SHIFT).unwrap();
        assert_eq!((key.key, key.modifiers, key.text.as_str()), (0x0100_0015, 0x0200_0000, ""));
        // Shift単体など
        assert!(qt_key_from_keysym(0xffe1, 0).is_none());
    }

    #[test]
    fn panel_labels_are_trimmed_and_aux_is_joined() {
        let text = |s: &str| vec![(s.to_owned(), 0)];
        let candidates = vec![("1. ".to_owned(), "天気 ".to_owned()), ("".to_owned(), "転機".to_owned())];
        let panel = panel_from_ui(text("[2/"), text("41]"), candidates, 1);
        assert_eq!(panel.candidates, ["1　天気", "転機"]);
        assert_eq!(panel.selected, Some(1));
        assert_eq!(panel.aux, "[2/41]");
        assert_eq!(panel_from_ui(vec![], vec![], vec![], -1).selected, None);
    }

    /// 動いているfcitx5との通信を確かめる。Mozcがオンの状態で
    /// `cargo test -p kakikotoba -- --ignored --test-threads=1` で実行する。確定はしない。
    #[test]
    #[ignore = "fcitx5とMozcが要る"]
    fn talks_to_running_fcitx5() {
        let (sender, events) = mpsc::channel();
        spawn(move |event| sender.send(event).unwrap());
        let wait = Duration::from_secs(2);
        let Event::Connected(client) = events.recv_timeout(wait).unwrap() else {
            panic!("最初のイベントは接続のはず");
        };
        client.focus_in();
        let press = |sym, code, text: &str| {
            let key = QtKey { key: sym as i32, modifiers: 0, text: text.to_owned() };
            client.press(NativeKey { sym, code, state: 0, time: 0 }, key).map_err(|_| "送信失敗").unwrap();
        };
        // 文字はMozcが処理してプリエディットになる
        press(0x61, 38, "a");
        let mut preedit = None;
        while preedit.is_none() {
            if let Event::Preedit(segments, cursor) = events.recv_timeout(wait).unwrap() {
                preedit = Some((segments, cursor));
            }
        }
        let (segments, cursor) = preedit.unwrap();
        assert_eq!(segments[0].0, "あ");
        assert_eq!(cursor, 3);
        client.reset();
        client.focus_out();
    }
}
