import QtCore
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Layouts
import Qt.labs.platform as Platform
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
        } else {
            finishSaving(editor.save());
        }
    }

    /// 失敗したら、保存の後に行うはずだった操作は取り消す
    function finishSaving(error) {
        if (succeeded(error)) {
            runPending();
        } else {
            pendingAction = null;
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

    /// 閉じたら編集領域へ入力を戻すメニュー
    component RestoringMenu: QQC2.Menu {
        // メニューの項目が開いたダイアログから入力を奪わないように
        onClosed: {
            if (![commandSearch, pageSettingsDialog, metadataDialog, unsavedDialog].some(dialog => dialog.visible)) {
                editor.forceActiveFocus();
            }
        }
    }

    /// メニュー項目の定義。各メニューはこれを参照するだけにする
    component Command: QQC2.Action {
        /// 表示とキー登録に使う。Actionのshortcutは使わない（GlobalItemの登録と競合する）
        property string keys
    }

    /// 検索バーのボタン。押しても入力先は検索欄のまま
    component FindButton: QQC2.ToolButton {
        id: findButton
        property string tip
        focusPolicy: Qt.NoFocus
        QQC2.ToolTip.text: findButton.tip
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
    }

    /// グローバルメニューの項目。キーの登録もここで行う
    component GlobalItem: Platform.MenuItem {
        required property Command command
        text: command.text
        enabled: command.enabled
        checkable: command.checkable
        checked: command.checked
        shortcut: command.keys
        onTriggered: command.trigger()
    }

    /// ショートカットを右端に示すウィンドウ内メニューの項目
    component CommandItem: QQC2.MenuItem {
        id: commandItem
        required property Command command
        action: command

        contentItem: RowLayout {
            spacing: Kirigami.Units.gridUnit
            QQC2.Label {
                Layout.fillWidth: true
                // 内容を差し替えると、チェックボックスの分の余白が自動では空かない
                Layout.leftMargin: commandItem.checkable && commandItem.indicator
                    ? commandItem.indicator.width + commandItem.spacing : 0
                text: commandItem.text
                enabled: commandItem.enabled
            }
            QQC2.Label {
                text: commandItem.command.keys
                enabled: commandItem.enabled
                opacity: 0.7
            }
        }
    }

    /// コマンド検索の対象
    readonly property var searchableCommands: [
        newCommand, openCommand, saveCommand, saveAsCommand, quitCommand,
        undoCommand, redoCommand, cutCommand, copyCommand, pasteCommand, selectAllCommand, findCommand,
        pageModeCommand, pageSettingsCommand, metadataCommand, highlightCommand, symbolFormsCommand
    ]

    Command {
        id: searchCommand
        text: "コマンドを検索…"
        keys: "Ctrl+Alt+I"
        onTriggered: commandSearch.open()
    }
    Command {
        id: newCommand
        text: "新規"
        keys: "Ctrl+N"
        onTriggered: root.afterConfirmingUnsaved(() => editor.newDocument())
    }
    Command {
        id: openCommand
        text: "開く…"
        keys: "Ctrl+O"
        onTriggered: root.afterConfirmingUnsaved(() => openDialog.open())
    }
    Command {
        id: saveCommand
        text: "保存"
        keys: "Ctrl+S"
        onTriggered: root.save()
    }
    Command {
        id: saveAsCommand
        text: "名前を付けて保存…"
        keys: "Ctrl+Shift+S"
        onTriggered: saveDialog.open()
    }
    Command {
        id: quitCommand
        text: "終了"
        keys: "Ctrl+Q"
        onTriggered: root.close()
    }
    Command {
        id: undoCommand
        text: "元に戻す"
        keys: "Ctrl+Z"
        enabled: editor.canUndo
        onTriggered: editor.undo()
    }
    Command {
        id: redoCommand
        text: "やり直す"
        keys: "Ctrl+Y"
        enabled: editor.canRedo
        onTriggered: editor.redo()
    }
    Command {
        id: cutCommand
        text: "切り取り"
        keys: "Ctrl+X"
        enabled: editor.hasSelection
        onTriggered: editor.cut()
    }
    Command {
        id: copyCommand
        text: "コピー"
        keys: "Ctrl+C"
        enabled: editor.hasSelection
        onTriggered: editor.copy()
    }
    Command {
        id: pasteCommand
        text: "貼り付け"
        keys: "Ctrl+V"
        enabled: !editor.pageMode
        onTriggered: editor.paste()
    }
    Command {
        id: selectAllCommand
        text: "すべて選択"
        keys: "Ctrl+A"
        enabled: !editor.pageMode
        onTriggered: editor.selectAll()
    }
    Command {
        id: findCommand
        text: "検索…"
        keys: "Ctrl+F"
        enabled: !editor.pageMode
        onTriggered: findBar.show()
    }
    Command {
        id: pageModeCommand
        text: editor.pageMode ? "編集モードへ" : "ページ表示"
        keys: "F5"
        onTriggered: editor.togglePageMode()
    }
    Command {
        id: pageSettingsCommand
        text: "ページ設定…"
        onTriggered: pageSettingsDialog.open()
    }
    Command {
        id: metadataCommand
        text: "文書情報…"
        onTriggered: {
            metadataDialog.subtitle = editor.metadataText() || "文書情報はありません";
            metadataDialog.open();
        }
    }
    Command {
        id: highlightCommand
        text: "記法ハイライト"
        checkable: true
        checked: settings.highlightEnabled
        onTriggered: {
            settings.highlightEnabled = checked;
            root.applyHighlightOptions();
        }
    }
    Command {
        id: symbolFormsCommand
        text: "PreTNFの記号形式"
        checkable: true
        checked: settings.symbolFormsEnabled
        enabled: settings.highlightEnabled
        onTriggered: {
            settings.symbolFormsEnabled = checked;
            root.applyHighlightOptions();
        }
    }

    Platform.MenuBar {
        Platform.Menu {
            title: "ファイル(&F)"
            GlobalItem { command: newCommand }
            GlobalItem { command: openCommand }
            Platform.MenuSeparator {}
            GlobalItem { command: saveCommand }
            GlobalItem { command: saveAsCommand }
            Platform.MenuSeparator {}
            GlobalItem { command: quitCommand }
        }
        Platform.Menu {
            title: "編集(&E)"
            GlobalItem { command: undoCommand }
            GlobalItem { command: redoCommand }
            Platform.MenuSeparator {}
            GlobalItem { command: cutCommand }
            GlobalItem { command: copyCommand }
            GlobalItem { command: pasteCommand }
            Platform.MenuSeparator {}
            GlobalItem { command: selectAllCommand }
            GlobalItem { command: findCommand }
        }
        Platform.Menu {
            title: "表示(&V)"
            GlobalItem { command: pageModeCommand }
            GlobalItem { command: pageSettingsCommand }
            GlobalItem { command: metadataCommand }
            GlobalItem { command: searchCommand }
            Platform.MenuSeparator {}
            GlobalItem { command: highlightCommand }
            GlobalItem { command: symbolFormsCommand }
        }
    }

    menuBar: QQC2.MenuBar {
        RestoringMenu {
            title: "ファイル(&F)"
            CommandItem { command: newCommand }
            CommandItem { command: openCommand }
            QQC2.MenuSeparator {}
            CommandItem { command: saveCommand }
            CommandItem { command: saveAsCommand }
            QQC2.MenuSeparator {}
            CommandItem { command: quitCommand }
        }
        RestoringMenu {
            title: "編集(&E)"
            CommandItem { command: undoCommand }
            CommandItem { command: redoCommand }
            QQC2.MenuSeparator {}
            CommandItem { command: cutCommand }
            CommandItem { command: copyCommand }
            CommandItem { command: pasteCommand }
            QQC2.MenuSeparator {}
            CommandItem { command: selectAllCommand }
            CommandItem { command: findCommand }
        }
        RestoringMenu {
            title: "表示(&V)"
            CommandItem { command: pageModeCommand }
            CommandItem { command: pageSettingsCommand }
            CommandItem { command: metadataCommand }
            CommandItem { command: searchCommand }
            QQC2.MenuSeparator {}
            CommandItem { command: highlightCommand }
            CommandItem { command: symbolFormsCommand }
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

        footer: QQC2.ToolBar {
            id: findBar

            property bool notFound: false

            function show() {
                const selected = editor.selectedText();
                if (selected && !selected.includes("\n")) {
                    findField.text = selected;
                }
                notFound = false;
                visible = true;
                findField.selectAll();
                findField.forceActiveFocus();
            }

            function hide() {
                visible = false;
                editor.forceActiveFocus();
            }

            function find(backward) {
                if (findField.text) {
                    notFound = !editor.findText(findField.text, backward);
                }
            }

            visible: false

            contentItem: RowLayout {
                spacing: Kirigami.Units.smallSpacing
                QQC2.TextField {
                    id: findField
                    Layout.fillWidth: true
                    placeholderText: "本文を検索…"
                    onTextEdited: findBar.notFound = false
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            findBar.find(event.modifiers & Qt.ShiftModifier);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape) {
                            findBar.hide();
                            event.accepted = true;
                        }
                    }
                }
                QQC2.Label {
                    visible: findBar.notFound
                    text: "見つかりません"
                }
                FindButton {
                    text: "←"
                    tip: "次を検索（Enter）"
                    enabled: findField.text !== ""
                    onClicked: findBar.find(false)
                }
                FindButton {
                    text: "→"
                    tip: "前を検索（Shift+Enter）"
                    enabled: findField.text !== ""
                    onClicked: findBar.find(true)
                }
                FindButton {
                    text: "✕"
                    tip: "閉じる（Esc）"
                    onClicked: findBar.hide()
                }
            }
        }

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
            onPageModeChanged: findBar.hide()

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: eventPoint => optionsMenu.popup(eventPoint.position.x, eventPoint.position.y)
            }

            RestoringMenu {
                id: optionsMenu

                CommandItem { command: cutCommand }
                CommandItem { command: copyCommand }
                CommandItem { command: pasteCommand }
                QQC2.MenuSeparator {}
                CommandItem { command: pageModeCommand }
            }
        }
    }

    // メニューに載らないキーだけ。他はGlobalItemが登録する
    Shortcut {
        sequence: "Ctrl+Shift+Z"
        onActivated: editor.redo()
    }
    Shortcut {
        sequence: "Esc"
        enabled: editor.pageMode
        onActivated: editor.togglePageMode()
    }

    Kirigami.SearchDialog {
        id: commandSearch

        /// 閉じた後に実行するコマンド。先に実行すると、開いたダイアログから入力を奪ってしまう
        property Command chosen: null

        function choose(command) {
            if (command.enabled) {
                chosen = command;
                close();
            }
        }

        parent: root.overlay
        searchFieldPlaceholderText: "コマンドを検索…"
        emptyText: "見つかりません"
        model: {
            const query = text.trim().toLowerCase();
            return root.searchableCommands.filter(command => command.text.toLowerCase().includes(query));
        }
        delegate: QQC2.ItemDelegate {
            id: commandRow
            required property Command modelData
            width: ListView.view.width
            enabled: modelData.enabled
            highlighted: ListView.isCurrentItem
            onClicked: commandSearch.choose(modelData)
            contentItem: RowLayout {
                spacing: Kirigami.Units.gridUnit
                QQC2.Label {
                    Layout.fillWidth: true
                    text: commandRow.modelData.text
                }
                QQC2.Label {
                    text: commandRow.modelData.checkable
                        ? (commandRow.modelData.checked ? "オン" : "オフ") : commandRow.modelData.keys
                    opacity: 0.7
                }
            }
        }
        onAccepted: {
            if (currentItem) {
                choose(currentItem.modelData);
            }
        }
        onClosed: {
            editor.forceActiveFocus();
            const command = chosen;
            chosen = null;
            if (command) {
                command.trigger();
            }
        }
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
        // 外側のクリックで閉じると、取り消しとして扱えない
        closePolicy: QQC2.Popup.CloseOnEscape
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
                onTriggered: unsavedDialog.reject()
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
        onAccepted: root.finishSaving(editor.saveAs(selectedFile))
        onRejected: root.pendingAction = null
    }
}
