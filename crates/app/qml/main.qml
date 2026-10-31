import QtCore
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import kakikotoba

Kirigami.ApplicationWindow {
    id: root

    /// 未保存の確認または保存先の選択が済んだ後に行う操作
    property var pendingAction: null
    property bool closeConfirmed: false

    title: (editor.modified ? "*" : "") + (editor.documentName || "無題")
        + (editor.pageMode ? " (" + editor.pageLabel + ")" : "") + " — kakikotoba"
    width: 1000
    height: 800

    function applyHighlightOptions() {
        editor.setHighlightOptions(settings.highlightEnabled, settings.symbolFormsEnabled);
    }

    function applyPageOptions() {
        const a6 = settings.pageFormat === 0;
        editor.setPageOptions(settings.pageFormat, a6 ? settings.a6Chars : settings.b6Chars,
                              a6 ? settings.a6Lines : settings.b6Lines, settings.spreadMode);
    }

    function succeeded(error) {
        if (error) {
            showPassiveNotification(error, "long");
        }
        return !error;
    }

    function runPending() {
        const action = pendingAction;
        pendingAction = null;
        if (action) {
            action();
        }
    }

    function save() {
        if (!editor.hasFile) {
            saveDialog.open();
        } else if (succeeded(editor.save())) {
            runPending();
        }
    }

    function afterConfirmingUnsaved(action) {
        pendingAction = action;
        if (editor.modified) {
            unsavedDialog.open();
        } else {
            runPending();
        }
    }

    onClosing: close => {
        if (editor.modified && !closeConfirmed) {
            close.accepted = false;
            afterConfirmingUnsaved(() => {
                closeConfirmed = true;
                root.close();
            });
        }
    }

    Settings {
        id: settings
        property bool highlightEnabled: true
        property bool symbolFormsEnabled: true
        /// 0: 擬A6, 1: 擬B6
        property int pageFormat: 0
        property int a6Chars: 39
        property int a6Lines: 17
        property int b6Chars: 43
        property int b6Lines: 18
        /// 0: 左ページ始まり, 1: 最初から見開き
        property int spreadMode: 0
        Component.onCompleted: {
            root.applyHighlightOptions();
            root.applyPageOptions();
        }
    }

    pageStack.initialPage: Kirigami.Page {
        padding: 0
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        VerticalEditor {
            id: editor
            anchors.fill: parent
            focus: true
            fillColor: Kirigami.Theme.backgroundColor
            textColor: Kirigami.Theme.textColor
            selectionColor: Kirigami.Theme.highlightColor
            delimiterColor: Kirigami.Theme.disabledTextColor
            tagColor: Kirigami.Theme.linkColor
            rubyColor: Kirigami.Theme.positiveTextColor
            emphasisColor: Kirigami.Theme.negativeTextColor
            sidelineColor: Kirigami.Theme.neutralTextColor
            commentColor: Kirigami.Theme.disabledTextColor
            strongColor: Kirigami.Theme.linkColor
            codeColor: Kirigami.Theme.visitedLinkColor
            paperColor: Kirigami.Theme.alternateBackgroundColor
            // Backspaceがウィンドウ側のショートカットに取られると、編集領域へ届かない
            Keys.onShortcutOverride: event => event.accepted = event.key === Qt.Key_Backspace

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: eventPoint => optionsMenu.popup(eventPoint.position.x, eventPoint.position.y)
            }

            QQC2.Menu {
                id: optionsMenu
                onClosed: editor.forceActiveFocus()

                QQC2.MenuItem {
                    text: editor.pageMode ? "編集モードへ (F5)" : "ページ表示 (F5)"
                    onTriggered: editor.togglePageMode()
                }
                QQC2.MenuItem {
                    text: "ページ設定…"
                    onTriggered: pageSettingsDialog.open()
                }
                QQC2.MenuItem {
                    text: "文書情報…"
                    onTriggered: {
                        metadataDialog.subtitle = editor.metadataText() || "文書情報はありません";
                        metadataDialog.open();
                    }
                }
                QQC2.MenuSeparator {}
                QQC2.MenuItem {
                    text: "記法ハイライト"
                    checkable: true
                    checked: settings.highlightEnabled
                    onTriggered: {
                        settings.highlightEnabled = checked;
                        root.applyHighlightOptions();
                    }
                }
                QQC2.MenuItem {
                    text: "PreTNFの記号形式"
                    checkable: true
                    checked: settings.symbolFormsEnabled
                    enabled: settings.highlightEnabled
                    onTriggered: {
                        settings.symbolFormsEnabled = checked;
                        root.applyHighlightOptions();
                    }
                }
            }
        }
    }

    Shortcut {
        sequences: [StandardKey.Open]
        onActivated: root.afterConfirmingUnsaved(() => openDialog.open())
    }
    Shortcut {
        sequences: [StandardKey.Save]
        onActivated: root.save()
    }
    Shortcut {
        sequences: [StandardKey.SaveAs]
        onActivated: saveDialog.open()
    }

    Shortcut {
        sequence: "F5"
        onActivated: editor.togglePageMode()
    }
    Shortcut {
        sequence: "Esc"
        enabled: editor.pageMode
        onActivated: editor.togglePageMode()
    }

    Kirigami.PromptDialog {
        id: metadataDialog
        title: "文書情報"
        standardButtons: QQC2.Dialog.Close
        onClosed: editor.forceActiveFocus()
    }

    Kirigami.Dialog {
        id: pageSettingsDialog
        title: "ページ設定"
        standardButtons: QQC2.Dialog.Close
        padding: Kirigami.Units.largeSpacing
        onClosed: editor.forceActiveFocus()

        GridLayout {
            columns: 2

            QQC2.Label { text: "版型" }
            QQC2.ComboBox {
                model: ["擬A6（文庫）", "擬B6"]
                currentIndex: settings.pageFormat
                onActivated: index => {
                    settings.pageFormat = index;
                    root.applyPageOptions();
                }
            }
            QQC2.Label { text: "字数" }
            QQC2.SpinBox {
                from: 10
                to: 80
                value: settings.pageFormat === 0 ? settings.a6Chars : settings.b6Chars
                onValueModified: {
                    if (settings.pageFormat === 0) {
                        settings.a6Chars = value;
                    } else {
                        settings.b6Chars = value;
                    }
                    root.applyPageOptions();
                }
            }
            QQC2.Label { text: "行数" }
            QQC2.SpinBox {
                from: 5
                to: 40
                value: settings.pageFormat === 0 ? settings.a6Lines : settings.b6Lines
                onValueModified: {
                    if (settings.pageFormat === 0) {
                        settings.a6Lines = value;
                    } else {
                        settings.b6Lines = value;
                    }
                    root.applyPageOptions();
                }
            }
            QQC2.Label { text: "見開き" }
            QQC2.ComboBox {
                model: ["左ページ始まり", "最初から見開き"]
                currentIndex: settings.spreadMode
                onActivated: index => {
                    settings.spreadMode = index;
                    root.applyPageOptions();
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: unsavedDialog
        title: "保存されていない変更があります"
        subtitle: "変更を保存しますか？"
        standardButtons: QQC2.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: "保存"
                onTriggered: {
                    unsavedDialog.close();
                    root.save();
                }
            },
            Kirigami.Action {
                text: "破棄"
                onTriggered: {
                    unsavedDialog.close();
                    root.runPending();
                }
            },
            Kirigami.Action {
                text: "キャンセル"
                onTriggered: unsavedDialog.close()
            }
        ]
        onRejected: root.pendingAction = null
        onClosed: editor.forceActiveFocus()
    }

    FileDialog {
        id: openDialog
        nameFilters: ["テキスト (*.txt *.md)", "すべてのファイル (*)"]
        onAccepted: root.succeeded(editor.open(selectedFile))
    }

    FileDialog {
        id: saveDialog
        fileMode: FileDialog.SaveFile
        nameFilters: ["テキスト (*.txt *.md)", "すべてのファイル (*)"]
        onAccepted: {
            if (root.succeeded(editor.saveAs(selectedFile))) {
                root.runPending();
            }
        }
        onRejected: root.pendingAction = null
    }
}
