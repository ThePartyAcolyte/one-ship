import QtQuick
import Quickshell
import qs.Commons
import qs.Ui

// Unified control panel for ONE-SHIP: displays live runtime telemetry from `oneship-status.json`
// in both production and development modes, plus production service actions or `run_dev.sh` unit controls.
Panel {
  id: root
  moduleName: "tech.kirulab.one-ship"
  manageIpc: false
  property var hostWidget: null
  property var anchorItem: null

  readonly property var status: hostWidget?.status ?? null
  readonly property var unit: hostWidget?.unit ?? null
  readonly property var runtime: hostWidget?.runtime ?? null
  readonly property var dev: hostWidget?.dev ?? null
  readonly property var units: hostWidget?.units ?? null
  readonly property var buildProgress: hostWidget?.buildProgress ?? null
  readonly property var guiProgress: hostWidget?.guiProgress ?? null

  readonly property bool isDevMode: hostWidget?.isDevMode ?? false
  readonly property bool unitInstalled: hostWidget?.prodUnitInstalled ?? false
  readonly property bool daemonRunning: hostWidget?.daemonRunning ?? false
  readonly property bool guiWindowOpen: hostWidget?.guiWindowOpen ?? false
  readonly property int workingCount: hostWidget?.workingAgentsCount ?? 0
  readonly property int idleCount: hostWidget?.idleAgentsCount ?? 0
  readonly property int errorCount: hostWidget?.errorAgentsCount ?? 0
  readonly property int servicesCount: hostWidget?.runningServicesCount ?? 0
  readonly property int approvalsCount: hostWidget?.pendingApprovalsCount ?? 0
  readonly property var activeAgents: hostWidget?.activeAgents ?? []

  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property color dim: Qt.darker(barForeground, 1.55)
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  property bool confirmingStop: false
  property bool confirmingStopAll: false
  onOpenedChanged: { confirmingStop = false; confirmingStopAll = false }
  Timer { id: confirmationTimeout; interval: 6000; onTriggered: root.confirmingStop = false }
  Timer { id: stopAllConfirmationTimeout; interval: 6000; onTriggered: root.confirmingStopAll = false }

  readonly property var unitRows: [
    { key: "daemon", logs: "daemon", label: "Daemon" },
    { key: "daemon_build", logs: "build", label: "Build" },
    { key: "daemon_watch", logs: "watch", label: "Watcher" },
    { key: "vite", logs: "vite", label: "Vite" },
    { key: "gui", logs: "gui", label: "Interface" }
  ]

  function requestDaemonStop() {
    if (confirmingStop) {
      if (isDevMode) hostWidget.runDev(["stop", "daemon"])
      else hostWidget.runProdAction("stop")
      confirmingStop = false
      confirmationTimeout.stop()
    } else {
      confirmingStop = true
      confirmationTimeout.restart()
    }
  }

  function requestStopAll() {
    if (confirmingStopAll) {
      hostWidget.runDev(["stop", "all"])
      confirmingStopAll = false
      stopAllConfirmationTimeout.stop()
    } else {
      confirmingStopAll = true
      stopAllConfirmationTimeout.restart()
    }
  }

  function metaText() {
    var mode = isDevMode ? "Development" : "Production"
    if (!daemonRunning) return mode + " · Stopped"
    if (runtime && runtime.mcp_port) return mode + " · :" + runtime.mcp_port
    return mode + " · Running"
  }

  function agentStatusText(agent) {
    var parts = [agent.status]
    if (agent.runner) parts.push(agent.runner)
    return parts.join(" · ")
  }

  function unitTitle(item) {
    if (item.key === "daemon_build") {
      var u = units ? units[item.key] : null
      if (u && u.active_state === "activating" && buildProgress) {
        if (buildProgress.phase === "cargo_build" && buildProgress.percent !== null)
          return item.label + " · " + buildProgress.percent + "%"
        if (buildProgress.phase === "export_bindings")
          return item.label + " · bindings"
      }
    }
    if (item.key === "gui") {
      var guiUnit = units ? units[item.key] : null
      if (guiUnit && guiUnit.active_state === "active" && !root.guiWindowOpen && guiProgress) {
        if (guiProgress.phase === "cargo_build" && guiProgress.percent !== null)
          return item.label + " · " + guiProgress.percent + "%"
      }
    }
    return item.label
  }

  function unitText(key) {
    var u = units ? units[key] : null
    if (!u) return "unknown"
    if (u.active_state === "failed") return "failed · " + u.result
    if (key === "daemon_build") {
      if (u.active_state === "activating" && buildProgress) {
        var elapsed = buildProgress.started_at_seconds
          ? Math.max(0, Math.floor(Date.now() / 1000) - buildProgress.started_at_seconds)
          : null
        if (buildProgress.phase === "cargo_build") {
          var count = buildProgress.total_count > 0 ? (buildProgress.completed_count + "/" + buildProgress.total_count + " · ") : ""
          var item = buildProgress.current_item ? (buildProgress.current_item + " · ") : ""
          return count + item + (elapsed !== null ? elapsed + "s" : "compiling")
        }
        if (buildProgress.phase === "export_bindings")
          return "exporting ts-rs bindings" + (elapsed !== null ? " · " + elapsed + "s" : "")
      }
      if (u.active_state === "inactive" && u.result === "success") {
        if (buildProgress && buildProgress.last_duration_seconds !== null)
          return "idle · last " + buildProgress.last_duration_seconds + "s"
        return "idle"
      }
    }
    if (key === "gui" && u.active_state === "active" && !root.guiWindowOpen) {
      if (guiProgress && guiProgress.phase === "cargo_build") {
        var gElapsed = guiProgress.started_at_seconds
          ? Math.max(0, Math.floor(Date.now() / 1000) - guiProgress.started_at_seconds)
          : null
        var gCount = guiProgress.total_count > 0 ? (guiProgress.completed_count + "/" + guiProgress.total_count + " · ") : ""
        var gItem = guiProgress.current_item ? (guiProgress.current_item + " · ") : ""
        return gCount + gItem + (gElapsed !== null ? gElapsed + "s" : "compiling")
      }
    }
    var text = u.active_state + (u.sub_state && u.sub_state !== u.active_state ? " (" + u.sub_state + ")" : "")
    if (u.main_pid !== null) text += " · PID " + u.main_pid
    if (key === "daemon" && dev && dev.daemon_listening_ports && dev.daemon_listening_ports.length > 0)
      text += " · :" + dev.daemon_listening_ports.join(", :")
    if (key === "vite" && dev && dev.vite_listening_ports && dev.vite_listening_ports.length > 0)
      text += " · :" + dev.vite_listening_ports.join(", :")
    if (key === "gui" && u.active_state === "active" && !root.guiWindowOpen) text += " · starting window"
    return text
  }

  component ActionButton: Button {
    foreground: root.barForeground
    accent: foreground
    bordered: true
    focusable: true
    fontFamily: root.fontFamily
    fontSize: Style.font.caption
    opacity: enabled ? 1 : 0.4
  }
  component Label: Text {
    width: parent.width
    color: root.barForeground
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
  }
  component SectionTitle: Label {
    font.pixelSize: Style.font.body
    font.bold: true
  }

  KeyboardPanel {
    anchorItem: root.anchorItem
    bar: root.bar
    owner: root.hostWidget
    open: root.opened
    focusTarget: content
    contentWidth: fittedContentWidth(Style.space(380))
    contentHeight: fittedContentHeight(column.implicitHeight)
    Flickable {
      id: content
      anchors.fill: parent
      clip: true
      contentWidth: width
      contentHeight: column.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      Keys.onEscapePressed: {
        if (root.confirmingStop) root.confirmingStop = false
        else if (root.confirmingStopAll) root.confirmingStopAll = false
        else root.close()
      }
      Column {
        id: column
        width: content.width
        spacing: Style.space(12)

        Row {
          width: parent.width
          spacing: Style.space(8)
          PanelHero {
            width: Math.max(0, parent.width - headerActions.width - parent.spacing)
            title: "ONE-SHIP"
            meta: root.metaText()
            foreground: root.barForeground
            fontFamily: root.fontFamily
            iconComponent: Component {
              Item {
                width: Style.font.display
                height: width
                OpticalGlyph { anchors.fill: parent; text: "\u{f0463}"; color: root.barForeground; fontFamily: root.fontFamily; fontSize: Style.font.display }
              }
            }
          }
          Row {
            id: headerActions
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(6)
            ActionButton {
              visible: !root.isDevMode
              text: root.guiWindowOpen ? "Focus" : "Open"
              onClicked: { root.hostWidget.openInterface(); root.close() }
            }
            ActionButton {
              visible: root.isDevMode
              text: "Restart"
              enabled: !(root.hostWidget?.devDaemonBuilding ?? false)
              onClicked: { root.hostWidget.runDev(["all"]); root.close() }
            }
            ActionButton {
              visible: root.isDevMode
              text: root.confirmingStopAll ? "Confirm" : "Stop"
              foreground: root.confirmingStopAll ? root.urgent : root.barForeground
              selected: root.confirmingStopAll
              enabled: (root.hostWidget?.devDaemonActive ?? false) || (root.hostWidget?.devGuiActive ?? false) || (root.hostWidget?.devDaemonBuilding ?? false)
              onClicked: root.requestStopAll()
            }
          }
        }

        Label {
          visible: root.confirmingStopAll
          text: "Stopping all stops the daemon, every supervised agent, Vite, and the interface."
          color: root.urgent
        }

        Label {
          visible: (root.hostWidget?.error ?? "") !== ""
          text: root.hostWidget?.error ?? ""
          color: root.urgent
        }

        Label {
          visible: root.approvalsCount > 0
          text: root.approvalsCount + " approval" + (root.approvalsCount === 1 ? "" : "s") + " waiting for review in ONE-SHIP."
          color: root.urgent
        }

        PanelSeparator { foreground: root.barForeground }

        Row {
          width: parent.width
          spacing: Style.space(14)
          Column {
            spacing: Style.space(2)
            SectionTitle { width: implicitWidth; text: String(root.workingCount) }
            Label { width: implicitWidth; text: "Working"; color: root.dim }
          }
          Column {
            spacing: Style.space(2)
            SectionTitle { width: implicitWidth; text: String(root.idleCount) }
            Label { width: implicitWidth; text: "Idle"; color: root.dim }
          }
          Column {
            spacing: Style.space(2)
            SectionTitle { width: implicitWidth; text: String(root.servicesCount) }
            Label { width: implicitWidth; text: "Services"; color: root.dim }
          }
          Column {
            spacing: Style.space(2)
            SectionTitle { width: implicitWidth; text: String(root.approvalsCount); color: root.approvalsCount > 0 ? root.urgent : root.barForeground }
            Label { width: implicitWidth; text: "Approvals"; color: root.approvalsCount > 0 ? root.urgent : root.dim }
          }
        }

        PanelSeparator { foreground: root.barForeground }
        SectionTitle { text: "Active Agents" }
        Label {
          visible: root.activeAgents.length === 0
          text: root.daemonRunning ? "No active agents running." : "Daemon is offline."
          color: root.dim
        }
        Repeater {
          model: root.activeAgents
          Column {
            required property var modelData
            width: column.width
            spacing: Style.space(1)
            Label {
              text: modelData.agent_id + " · " + modelData.project_name
              font.bold: true
            }
            Label {
              text: root.agentStatusText(modelData)
              color: modelData.status === "failed" ? root.urgent : root.dim
            }
          }
        }

        PanelSeparator { foreground: root.barForeground }
        SectionTitle { text: root.isDevMode ? "Dev Daemon & Interface" : "Daemon & Interface" }
        Flow {
          width: parent.width
          spacing: Style.space(6)
          ActionButton {
            text: root.daemonRunning ? "Restart Daemon" : "Start Daemon"
            enabled: root.isDevMode ? !(root.hostWidget?.devDaemonBuilding ?? false) : root.unitInstalled
            onClicked: {
              if (root.isDevMode) root.hostWidget.runDev(["daemon"])
              else root.hostWidget.runProdAction(root.daemonRunning ? "restart" : "start")
            }
          }
          ActionButton {
            text: root.confirmingStop ? "Confirm Stop" : "Stop Daemon"
            foreground: root.confirmingStop ? root.urgent : root.barForeground
            selected: root.confirmingStop
            enabled: root.isDevMode ? (root.hostWidget?.devDaemonActive ?? false) : (root.unitInstalled && root.daemonRunning)
            onClicked: root.requestDaemonStop()
          }
          ActionButton {
            visible: root.confirmingStop
            text: "Cancel"
            onClicked: root.confirmingStop = false
          }
          ActionButton {
            visible: root.isDevMode
            text: root.guiWindowOpen ? "Focus" : ((root.hostWidget?.devGuiBuilding ?? false) ? "Starting..." : "Open")
            enabled: !(root.hostWidget?.devDaemonBuilding ?? false)
            onClicked: { root.hostWidget.openInterface(); root.close() }
          }
          ActionButton {
            visible: !root.isDevMode
            text: "Logs"
            enabled: root.unitInstalled
            onClicked: root.hostWidget.runProdAction("logs")
          }
          ActionButton {
            text: "Close GUI"
            enabled: root.isDevMode ? (root.hostWidget?.devGuiActive ?? false) : root.guiWindowOpen
            onClicked: {
              if (root.isDevMode) root.hostWidget.runDev(["stop", "gui"])
              else root.hostWidget.runProdAction("close-gui")
            }
          }
        }
        Label {
          visible: root.confirmingStop
          text: "Stopping the daemon stops every supervised agent and service."
          color: root.urgent
        }
        Label {
          visible: !root.isDevMode && !root.unitInstalled
          text: "oneship-daemon.service not installed (run ./install.sh)."
          color: root.dim
        }

        PanelSeparator { visible: root.isDevMode; foreground: root.barForeground }
        Repeater {
          model: root.isDevMode ? root.unitRows : []
          Column {
            id: row
            required property var modelData
            readonly property var u: root.units ? root.units[modelData.key] : null
            readonly property bool failed: u !== null && u.active_state === "failed"
            readonly property bool showBuildProgress: row.modelData.key === "daemon_build" && u !== null && u.active_state === "activating"
            width: column.width
            spacing: Style.space(4)
            Row {
              width: parent.width
              spacing: Style.space(8)
              Column {
                width: Math.max(0, parent.width - logsButton.width - parent.spacing)
                spacing: Style.space(2)
                SectionTitle { text: root.unitTitle(row.modelData) }
                Label { text: root.unitText(row.modelData.key); color: row.failed ? root.urgent : root.dim }
              }
              ActionButton {
                id: logsButton
                text: "Logs"
                onClicked: root.hostWidget.runDev(["logs", row.modelData.logs])
              }
            }
            Rectangle {
              visible: row.showBuildProgress
              width: parent.width
              height: 2
              radius: 1
              color: Qt.rgba(root.barForeground.r, root.barForeground.g, root.barForeground.b, 0.18)
              Rectangle {
                readonly property real progressRatio: (root.buildProgress && root.buildProgress.phase === "export_bindings") ? 1
                  : (root.buildProgress && root.buildProgress.percent !== null) ? Math.max(0.03, Math.min(1, root.buildProgress.percent / 100))
                  : 0.03
                width: parent.width * progressRatio
                height: parent.height
                radius: 1
                color: root.barForeground
                Behavior on width { NumberAnimation { duration: 250 } }
              }
            }
            Label {
              visible: row.failed && row.u.log_tail !== null
              text: row.failed && row.u.log_tail !== null ? row.u.log_tail.join("\n") : ""
              color: root.dim
              wrapMode: Text.WrapAnywhere
            }
          }
        }

        PanelSeparator { foreground: root.barForeground }
        Label {
          text: (root.isDevMode ? "dev units + oneship-status.json" : "oneship-daemon.service + oneship-status.json") + " · refreshes every " + (root.hostWidget?.pollIntervalMs ?? 0) + " ms"
          color: root.dim
        }
      }
    }
  }
}
