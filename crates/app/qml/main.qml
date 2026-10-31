import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import kakikotoba

Kirigami.ApplicationWindow {
    id: root

    /// 未保存の確認または保存先の選択が済んだ後に行う操作
    property var pendingAction: null
    property bool closeConfirmed: false

    title: (editor.modified ? "*" : "") + (editor.documentName || "無題") + " — kakikotoba"
    width: 1000
    height: 800

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
            // Backspaceがウィンドウ側のショートカットに取られると、編集領域へ届かない
            Keys.onShortcutOverride: event => event.accepted = event.key === Qt.Key_Backspace
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
