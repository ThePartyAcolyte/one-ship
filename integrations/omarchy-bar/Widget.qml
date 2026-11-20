import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// ONE-SHIP unified Omarchy bar widget (Spec 12 §7.3 & Spec 41 Phase 4).
// Always reads `$XDG_RUNTIME_DIR/oneship-status.json` for live agent/approval/service telemetry.
// Auto-detects `scripts/run_dev.sh` when symlinked from a source checkout: displays Cargo build
// progress and `oneship-dev-*` unit controls when dev units are active or production unit is absent,
// and falls back cleanly to production `oneship-daemon.service` controls on installed systems.
BarWidget {
  id: root
  moduleName: "tech.kirulab.one-ship"

  readonly property int pollIntervalMs: setting("pollIntervalMs", 2000)
  readonly property string pluginDir: Qt.resolvedUrl(".").toString().replace(/^file:\/\//, "").replace(/\/$/, "")
  property string devLauncher: ""
  property var status: null
  property string error: ""

  readonly property var unit: status ? status.unit : null
  readonly property var runtime: status ? status.runtime : null
  readonly property var dev: status ? (status.dev ?? null) : null
  readonly property var units: dev ? (dev.units ?? null) : null
  readonly property var buildProgress: dev ? (dev.build_progress ?? null) : null
  readonly property var guiProgress: dev ? (dev.gui_progress ?? null) : null

  readonly property bool prodUnitInstalled: unit !== null && unit.exists === true
  readonly property bool prodUnitActive: unit !== null && unit.active_state === "active"
  readonly property bool prodUnitFailed: unit !== null && unit.active_state === "failed"

  readonly property bool devDaemonActive: units !== null && units.daemon.active_state === "active"
  readonly property bool devDaemonListening: devDaemonActive && (dev && dev.daemon_listening_ports ? dev.daemon_listening_ports.length : 0) > 0
  readonly property bool devDaemonBuilding: units !== null && units.daemon_build.active_state === "activating"
  readonly property bool devGuiActive: units !== null && units.gui.active_state === "active"
  readonly property bool devGuiWindowOpen: dev !== null && dev.gui_window_pid !== null
  readonly property bool devGuiBuilding: devGuiActive && !devGuiWindowOpen
  readonly property bool devAnyBusy: units !== null && (
    devDaemonActive ||
    devDaemonBuilding ||
    units.daemon_build.active_state === "failed" ||
    units.daemon_watch.active_state === "active" ||
    units.vite.active_state === "active" ||
    devGuiActive ||
    units.gui.active_state === "failed"
  )
  readonly property bool devAnyFailed: units !== null && Object.keys(units).some(function(k) { return units[k].active_state === "failed" })

  // Switch seamlessly between dev harness mode and production service mode.
  readonly property bool isDevMode: devLauncher !== "" && (devAnyBusy || !prodUnitActive)

  readonly property bool daemonRunning: isDevMode
    ? devDaemonActive
    : (status !== null && (status.daemon_pid !== null || prodUnitActive))
  readonly property bool guiWindowOpen: isDevMode
    ? devGuiWindowOpen
    : (status !== null && status.gui_window_pid !== null)

  readonly property int workingAgentsCount: runtime ? (runtime.working_agents_count ?? 0) : 0
  readonly property int idleAgentsCount: runtime ? (runtime.idle_agents_count ?? 0) : 0
  readonly property int blockedAgentsCount: runtime ? (runtime.blocked_agents_count ?? 0) : 0
  readonly property int errorAgentsCount: runtime ? (runtime.error_agents_count ?? 0) : 0
  readonly property int activeAgentsCount: runtime ? (runtime.active_agents_count ?? 0) : 0
  readonly property int runningServicesCount: runtime ? (runtime.running_services_count ?? 0) : 0
  readonly property int pendingApprovalsCount: runtime ? (runtime.pending_approvals_count ?? 0) : 0
  readonly property var activeAgents: runtime && runtime.active_agents ? runtime.active_agents : []

  readonly property bool isBuilding: isDevMode && (devDaemonBuilding || devGuiBuilding)
  readonly property bool hasAlert: error !== "" || (isDevMode ? devAnyFailed : prodUnitFailed) || errorAgentsCount > 0 || pendingApprovalsCount > 0
  readonly property string badgeText: pendingApprovalsCount > 0 ? "!"
    : (workingAgentsCount > 0 ? String(workingAgentsCount) : "")

  readonly property bool opened: panel.opened
  readonly property bool popoutSwitchClosing: panel.popoutSwitchClosing
  readonly property real openPanelIndicatorWidth: Style.bar.iconCanvas
  readonly property real openPanelIndicatorHeight: Style.bar.iconCanvas
  function open() { panel.open() }
  function close() { panel.close() }
  function closeForPopoutSwitch() { panel.closeForPopoutSwitch() }

  function runDev(args) {
    if (devLauncher === "") return
    Quickshell.execDetached([devLauncher].concat(args))
    poll.restart()
  }

  function openInterface() {
    if (isDevMode) {
      runDev(["gui"])
      return
    }
    if (guiWindowOpen && status && status.gui_window_pid) {
      Quickshell.execDetached(["bash", "-c", "if command -v hyprctl >/dev/null 2>&1; then hyprctl dispatch focuswindow \"pid:$0\" >/dev/null; fi", String(status.gui_window_pid)])
      return
    }
    Quickshell.execDetached(["bash", "-c", "if command -v one-ship-launch >/dev/null 2>&1; then exec one-ship-launch; elif command -v one-ship >/dev/null 2>&1; then exec one-ship; fi"])
    poll.restart()
  }

  function runProdAction(action) {
    if (action === "start") {
      Quickshell.execDetached(["systemctl", "--user", "start", "oneship-daemon.service"])
    } else if (action === "restart") {
      Quickshell.execDetached(["systemctl", "--user", "restart", "oneship-daemon.service"])
    } else if (action === "stop") {
      Quickshell.execDetached(["systemctl", "--user", "stop", "oneship-daemon.service"])
    } else if (action === "close-gui") {
      Quickshell.execDetached(["pkill", "-TERM", "-x", "one-ship"])
    } else if (action === "logs") {
      Quickshell.execDetached(["bash", "-c", "cmd='journalctl --user -u oneship-daemon.service -f -n 200'; for t in ghostty kitty alacritty foot wezterm gnome-terminal konsole xfce4-terminal x-terminal-emulator xterm; do if command -v \"$t\" >/dev/null 2>&1; then case \"$t\" in wezterm) exec \"$t\" start -- $cmd ;; gnome-terminal) exec \"$t\" -- $cmd ;; foot) exec \"$t\" $cmd ;; *) exec \"$t\" -e $cmd ;; esac; fi; done"])
    }
    poll.restart()
  }

  function stateText() {
    if (error !== "") return error
    if (isDevMode) {
      if (devAnyFailed) return "A dev unit failed"
      if (devDaemonBuilding) {
        if (buildProgress && buildProgress.phase === "cargo_build" && buildProgress.percent !== null)
          return "Building daemon (" + buildProgress.percent + "%)"
        if (buildProgress && buildProgress.phase === "export_bindings")
          return "Exporting ts-rs bindings"
        return "Building daemon"
      }
      if (devGuiBuilding) {
        if (guiProgress && guiProgress.phase === "cargo_build" && guiProgress.percent !== null)
          return "Building interface (" + guiProgress.percent + "%)"
        return "Starting interface"
      }
      if (!devDaemonActive) return "Dev daemon stopped"
      if (!devDaemonListening) return "Dev daemon starting"
    } else {
      if (prodUnitFailed) return "Daemon unit failed"
      if (!daemonRunning) return "Daemon stopped"
    }

    var parts = []
    if (workingAgentsCount > 0) parts.push(workingAgentsCount + " working")
    if (idleAgentsCount > 0) parts.push(idleAgentsCount + " idle")
    if (pendingApprovalsCount > 0) parts.push(pendingApprovalsCount + " approval" + (pendingApprovalsCount === 1 ? "" : "s"))
    if (runningServicesCount > 0) parts.push(runningServicesCount + " service" + (runningServicesCount === 1 ? "" : "s"))
    if (parts.length === 0) return guiWindowOpen ? "Running · interface open" : "Running · idle"
    return parts.join(" · ")
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    useActiveColor: false
    labelVisible: false
    hasVisualContent: true
    fixedWidth: root.vertical ? -1 : Style.bar.iconSlot
    fixedHeight: root.vertical ? Style.bar.iconSlot : -1
    tooltipText: (root.isDevMode ? "ONE-SHIP Dev · " : "ONE-SHIP · ") + root.stateText()
    onPressed: function(b) {
      if (b === Qt.LeftButton) root.openInterface()
      else if (b === Qt.RightButton) panel.toggle()
    }
    Item {
      anchors.centerIn: parent
      width: Style.bar.iconCanvas
      height: width
      opacity: root.daemonRunning || root.isBuilding || root.hasAlert ? 1 : 0.38
      Behavior on opacity { NumberAnimation { duration: 150 } }
      SequentialAnimation on opacity {
        running: root.isBuilding
        loops: Animation.Infinite
        NumberAnimation { from: 1.0; to: 0.45; duration: 650; easing.type: Easing.InOutSine }
        NumberAnimation { from: 0.45; to: 1.0; duration: 650; easing.type: Easing.InOutSine }
      }
      OpticalGlyph {
        anchors.fill: parent
        text: "\u{f0463}"
        color: root.hasAlert ? root.bar.urgent
          : (root.isBuilding || root.workingAgentsCount > 0) ? "#f59e0b"
          : button.foreground
        fontFamily: button.fontFamily
        fontSize: Style.bar.iconFont
      }
      Rectangle {
        visible: root.badgeText !== ""
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: -2
        anchors.bottomMargin: -2
        width: Math.max(11, badgeLabel.implicitWidth + 4)
        height: 11
        radius: 2
        color: root.pendingApprovalsCount > 0 ? root.bar.urgent : "#f59e0b"
        Text {
          id: badgeLabel
          anchors.centerIn: parent
          text: root.badgeText
          color: "#09090b"
          font.family: button.fontFamily
          font.pixelSize: 8
          font.bold: true
          textFormat: Text.PlainText
        }
      }
    }
  }

  ControlPanel { id: panel; bar: root.bar; hostWidget: root; anchorItem: button }

  // Optional dev harness discovery: absent on production installs, present when symlinked from repo.
  Process {
    id: resolver
    command: ["bash", "-c", "d=$(readlink -f -- \"$0\" 2>/dev/null || true) && f=\"$d/../../scripts/run_dev.sh\" && [[ -x $f ]] && realpath -- \"$f\" || true", root.pluginDir]
    running: true
    stdout: StdioCollector {
      onStreamFinished: {
        root.devLauncher = text.trim()
        poll.running = true
      }
    }
  }

  Process {
    id: statusProcess
    command: [
      "bash", "-c",
      "dev_launcher=\"$0\"; " +
      "runtime_file=\"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/oneship-status.json\"; " +
      "runtime_json=\"null\"; " +
      "if [[ -r $runtime_file ]]; then runtime_json=$(jq -c . \"$runtime_file\" 2>/dev/null || echo null); fi; " +
      "active=$(systemctl --user show oneship-daemon.service -p ActiveState --value 2>/dev/null || echo unknown); " +
      "sub=$(systemctl --user show oneship-daemon.service -p SubState --value 2>/dev/null || echo unknown); " +
      "res=$(systemctl --user show oneship-daemon.service -p Result --value 2>/dev/null || echo unknown); " +
      "unit_pid=$(systemctl --user show oneship-daemon.service -p MainPID --value 2>/dev/null || echo 0); " +
      "unit_exists=$(systemctl --user list-unit-files oneship-daemon.service --no-legend 2>/dev/null | wc -l); " +
      "daemon_pid=$(pgrep -x oneship-daemon | head -n 1); " +
      "gui_pid=$(pgrep -x one-ship | head -n 1); " +
      "dev_json=\"null\"; " +
      "if [[ -n $dev_launcher && -x $dev_launcher ]]; then dev_json=$(\"$dev_launcher\" status 2>/dev/null || echo null); fi; " +
      "jq -cn --arg active \"$active\" --arg sub \"$sub\" --arg res \"$res\" " +
      "--argjson unit_pid \"${unit_pid:-0}\" --argjson unit_exists \"${unit_exists:-0}\" " +
      "--arg daemon_pid \"$daemon_pid\" --arg gui_pid \"$gui_pid\" " +
      "--argjson runtime \"$runtime_json\" --argjson dev \"$dev_json\" '{ " +
      "  unit: { exists: ($unit_exists > 0), active_state: $active, sub_state: $sub, result: $res, main_pid: (if $unit_pid > 0 then $unit_pid else null end) }, " +
      "  daemon_pid: (if $daemon_pid == \"\" then null else ($daemon_pid | tonumber) end), " +
      "  gui_window_pid: (if $gui_pid == \"\" then null else ($gui_pid | tonumber) end), " +
      "  runtime: $runtime, " +
      "  dev: $dev " +
      "}'",
      root.devLauncher
    ]
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          root.status = JSON.parse(text)
          root.error = ""
        } catch (e) {
          root.status = null
          root.error = "Unable to read ONE-SHIP status"
        }
      }
    }
    stderr: StdioCollector { onStreamFinished: if (text.trim()) console.warn("ONE-SHIP status:", text.trim()) }
  }

  Timer {
    id: poll
    interval: root.pollIntervalMs
    repeat: true
    running: false
    triggeredOnStart: true
    onTriggered: if (!statusProcess.running) statusProcess.running = true
  }
}
