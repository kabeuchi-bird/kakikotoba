import QtQuick
import org.kde.kirigami as Kirigami
import kakikotoba

Kirigami.ApplicationWindow {
    title: "kakikotoba"
    width: 1000
    height: 800

    pageStack.initialPage: Kirigami.Page {
        padding: 0
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        VerticalEditor {
            anchors.fill: parent
            focus: true
            fillColor: Kirigami.Theme.backgroundColor
            textColor: Kirigami.Theme.textColor
            selectionColor: Kirigami.Theme.highlightColor
        }
    }
}
