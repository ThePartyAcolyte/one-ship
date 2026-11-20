import { createI18n } from 'vue-i18n';

const messages = {
  en: {
    hub: {
      title: "Project Tracker // Hub"
    },
    status: {
      nominal: "All nominal",
      working: "Working...",
      degraded: "Degraded",
      crashed: "Crashed"
    },
    action: {
      start: "Launch",
      stop: "Kill",
      restart: "Restart",
      inspect: "Inspect",
      agent_consult: "Consult Agent"
    },
    services: {
      start: "Start",
      stop: "Stop",
      running: "running",
      stopped: "stopped",
      fleet: "Active Services",
      configured: "configured"
    },
    agent: {
      connected: "CONNECTED",
      idle: "IDLE",
      unlinked: "Unlinked",
      working: "working",
      done: "done",
      input: "input",
      fail: "fail",
      launch: "launch"
    },
    settings: {
      global: "Global Settings",
      project: "Project Settings"
    },
    vcs: {
      action: "VCS Action",
      voice_action: "Voice VCS Action"
    },
    card: {
      remove_project: "Remove Project",
      cancel: "Cancel",
      purge_title: "PURGE PROJECT REGISTRY",
      unregister_title: "UNREGISTER FROM HUB (Database Only)",
      unregister_desc: "Removes project from ONE-SHIP registry. Keeps rula.toml, platform skills, and .agents/ untouched for future re-linking.",
      space_him_title: "SPACE HIM (De-integrate & Clean Kapawu Traces)",
      space_him_desc: "Removes project from registry AND surgically deletes rula.toml, platform skills, and registered personas. Preserves user code and custom skills.",
      type_to_confirm: "Type the project name to confirm:",
      purge_action: "Purge Registry",
      space_him_action: "Space Him"
    },
    diagnostics: {
      title: "System Diagnostics",
      ok: "OK",
      missing: "MISSING",
      unauth: "UNAUTH",
      offline: "OFFLINE"
    },
    prompt: {
      confirm: "Understood?"
    },
    error: {
      unknown: "Unknown Error"
    },
    empty: {
      services: "No active services",
      track_new: "Track New Project",
      track_desc: "Select a directory to initialize a project tracker. A rula.toml will be generated in the root."
    },
    metrics: {
      cpu: "CPU",
      ram: "MEM",
      swap: "SWP",
      disk: "DSK",
      net: "NET"
    },
    log: {
      title: "SYSTEM LOGS",
      view_history: "View Log History",
      close: "Close Log",
      end_of_logs: "-- End of loaded logs --"
    }
  },
  belta_translated: {
    hub: {
      title: "ONE-SHIP // Stashang"
    },
    status: {
      nominal: "Kowlting gonya gut (Everything is gonna be good)",
      working: "Mi du work, bosmang (I am working, boss)",
      degraded: "Xianlang: Kapawu no gut (Danger: Ship no good)",
      crashed: "Pashangfong! Kapawu det (F***! Ship dead)"
    },
    action: {
      start: "Gowork (Go to work)",
      stop: "Kwit (Quit)",
      restart: "Mo du (Do more/again)",
      inspect: "Mera (Look/Watch)",
      agent_consult: "Kowting Kopeng (Everything Friend)"
    },
    services: {
      start: "Gowork (Start)",
      stop: "Kwit (Stop)",
      running: "du work (running)",
      stopped: "det (stopped)",
      fleet: "Kapawulowda (Ships / Fleet)",
      configured: "rula-set (configured)"
    },
    agent: {
      connected: "KOPENG-LINK (Connected)",
      idle: "PAXON (Idle)",
      unlinked: "Na Link (Unlinked)",
      working: "du work",
      done: "finyish",
      input: "pensa",
      fail: "kaka",
      launch: "launch"
    },
    settings: {
      global: "Kowl Rula (Global Rules)",
      project: "Kapawu Rula (Project Rules)"
    },
    vcs: {
      action: "Kontów Ta-Du (Commit Action)",
      voice_action: "Weting Kontów (Voice Commit)"
    },
    card: {
      remove_project: "Klin Kapawu (Remove Ship)",
      cancel: "Nating (Cancel)",
      purge_title: "KLIN KAPAWU REGISTRY",
      unregister_title: "KOPENG-OUT (Database Only)",
      unregister_desc: "Remove ship from ONE-SHIP registry. Keep rula.toml and skills on disk.",
      space_him_title: "SPACE HIM (Launch to Vacuum)",
      space_him_desc: "Remove ship from registry AND purge rula.toml and Kapawu platform skills. User code is safe.",
      type_to_confirm: "Type ship name to confirm:",
      purge_action: "Purge",
      space_him_action: "Space Him!"
    },
    diagnostics: {
      title: "Mera Kapawu (Inspect Ship)",
      ok: "Gut (OK)",
      missing: "Kuxaku (Empty / Missing)",
      unauth: "Na Sasa (Unknown / Unauth)",
      offline: "Det (Dead / Offline)"
    },
    prompt: {
      confirm: "Sasa ke? (Do you know?)"
    },
    error: {
      unknown: "Na sasa (Not know)"
    },
    empty: {
      services: "Kuxaku. Na kapawu du work (Vacuum. No ship working)",
      track_new: "Mo Kapawu (More Ship)",
      track_desc: "Da kuxaku na im enemy; im home. Select a directory to initialize a rula.toml."
    },
    metrics: {
      cpu: "KORE",
      ram: "MEM",
      swap: "SWP",
      disk: "DSK",
      net: "NET"
    },
    log: {
      title: "KORE LOGIM (System Logs)",
      view_history: "Mera Logim (View Log History)",
      close: "Klin Mera (Close Log)",
      end_of_logs: "-- Finyish logim --"
    }
  },
  belta_pure: {
    hub: {
      title: "ONE-SHIP // Stashang"
    },
    status: {
      nominal: "Kowlting gonya gut",
      working: "Mi du work, bosmang",
      degraded: "Xianlang: Kapawu no gut",
      crashed: "Pashangfong! Kapawu det"
    },
    action: {
      start: "Gowork",
      stop: "Kwit",
      restart: "Mo du",
      inspect: "Mera",
      agent_consult: "Kowting Kopeng"
    },
    services: {
      start: "Gowork",
      stop: "Kwit",
      running: "du work",
      stopped: "det",
      fleet: "Kapawulowda",
      configured: "rula-set"
    },
    agent: {
      connected: "KOPENG-LINK",
      idle: "PAXON",
      unlinked: "Na Link",
      working: "du work",
      done: "finyish",
      input: "pensa",
      fail: "kaka",
      launch: "launch"
    },
    settings: {
      global: "Kowl Rula",
      project: "Kapawu Rula"
    },
    vcs: {
      action: "Kontów Ta-Du",
      voice_action: "Weting Kontów"
    },
    card: {
      remove_project: "Klin Kapawu",
      cancel: "Nating",
      purge_title: "KLIN KAPAWU REGISTRY",
      unregister_title: "KOPENG-OUT",
      unregister_desc: "Remove ship from registry. Keep rula.toml and skills on disk.",
      space_him_title: "SPACE HIM",
      space_him_desc: "Remove ship from registry AND purge rula.toml and Kapawu platform skills.",
      type_to_confirm: "Type ship name:",
      purge_action: "Purge",
      space_him_action: "Space Him!"
    },
    diagnostics: {
      title: "Mera Kapawu",
      ok: "Gut",
      missing: "Kuxaku",
      unauth: "Na Sasa",
      offline: "Det"
    },
    prompt: {
      confirm: "Sasa ke?"
    },
    error: {
      unknown: "Na sasa"
    },
    empty: {
      services: "Kuxaku. Na kapawu du work",
      track_new: "Mo Kapawu",
      track_desc: "Da kuxaku na im enemy; im home. Select a directory to initialize a rula.toml."
    },
    metrics: {
      cpu: "KORE",
      ram: "MEM",
      swap: "SWP",
      disk: "DSK",
      net: "NET"
    },
    log: {
      title: "KORE LOGIM",
      view_history: "Mera Logim",
      close: "Klin Mera",
      end_of_logs: "-- Finyish logim --"
    }
  }
};

export const i18n = createI18n({
  legacy: false, // Use Composition API
  locale: 'en', // default locale
  fallbackLocale: 'en',
  messages,
});
