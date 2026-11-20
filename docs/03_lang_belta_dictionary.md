# Lang Belta Dictionary: Software & Cockpit Lexicon
**Ecosystem:** ONE-SHIP (`one-ship`)  
**Context:** i18n localization, system terminology, diagnostics, and UI strings inspired by *Lang Belta* (Belter Creole from *The Expanse*).

> [!NOTE]
> **Status: DEFERRED UNTIL PACKAGING (Post-Completion Translation Pass)**
> Active development of ONE-SHIP's codebase, UI, and technical specifications is conducted strictly in English (`AGENTS.md` §1.2). This dictionary and the Cinturonero translation protocol ([Localization and Belta Language Strategy](specs/07-ui/06-localization-and-belta-language.md)) are parked until the application reaches functional completion and packaging, at which point localization will be executed as a dedicated, separate pass.

---

## 1. Core Philosophy: The ONE-SHIP Doctrine

In Belter (*Beltalowda*) culture, the fundamental survival principle is the **Doctrine of the One Ship**:

> *"As cells are separate but also make a body, so a ship is made from a crew, and a people from their ships. Only one ship."*  
> — Naomi Nagata / Michio Pa

In the context of **ONE-SHIP**:
* The workspace and its ecosystem are not isolated processes — they are a single vessel (**ONE-SHIP**).
* The interface is the command bridge (**Stashang** / **Cockpit**).
* The supervisor daemon is the **Bosmang**.
* AI agents and tools are the crew (**Kopenglowda**).
* A crashed service or bug is not an isolated event — it is a hull breach affecting the entire ship.

---

## 2. Entity Mapping (Ubiquitous Language)

Direct mapping of the concepts defined in [`01_ubiquitous_language.md`](01_ubiquitous_language.md) into *Lang Belta*:

| Original Term (DDD) | Lang Belta | Pronunciation / Etymology | System Role |
| :--- | :--- | :--- | :--- |
| **Hub** | **Kapawu** / **Stashang** | *ka-PA-wu* (from Malay *kapal*, ship) / *sta-SHANG* (station) | The main GUI window; the project's command bridge. |
| **Daemon** | **Bosmang** | *BOS-mang* (from English *boss-man*) | The background Rust process that supervises and orchestrates everything. |
| **Tray Hub** | **Lek Stashang** | *lek* (Dutch/Afrikaans *klein*, small) + *stashang* | Compact status menu in the system tray / bar. |
| **Tracker Config** | **Rula** / **Plan** | *RU-la* (rules / configuration) | Local project configuration file (`rula.toml`). |
| **Service / Subprocess** | **Kapawulowda** | *ka-pa-wu-LOW-da* (collective plural suffix) | Each individual service or command launched by the Daemon. |
| **Error Curator** | **Kaka-Tresh** | *KA-ka* (junk/error) + *tresh* (from English *trash*) | Subsystem that filters `stderr`, strips noise, and isolates failures. |
| **Core Error** | **Kore Xianlang** | *kore* (core) + *SHAN-lang* (Chinese 险浪, danger/alert) | The clean, synthesized error ready for LLM inspection. |
| **Agent Link** | **Kopeng-Link** | *KO-peng* (friend/crewmate) | Direct channel to an agent persona. |
| **Developer / User** | **Pampa** / **Beratna** | *PAM-pa* (veteran/elder) / *be-RAT-na* (brother) | The engineer at the helm. |

---

## 3. System States (Telemetry & Health Checks)

Standard terms for badges, service health indicators, and console outputs:

| Dev State | Lang Belta | Tone / UI | Literal Meaning | Console / UI Message |
| :--- | :--- | :--- | :--- | :--- |
| **Healthy / OK** | **Gut** | Green | Good / Nominal | `[GUT] Kapawu operational` |
| **All nominal** | **Kowlting gut** | Green | Everything is good | `Kowlting gonya gut` (Everything is going to be good) |
| **Running** | **Du work** | Blue / Cyan | Doing work | `[DU WORK] Process active on PID 4120` |
| **Idle / Standby** | **Paxon** | Gray / Cyan | Peace / Calm | `[PAXON] Awaiting commands...` |
| **Empty / Unset** | **Kuxaku** | Gray | Empty / Void | `[KUXAKU] No service assigned` |
| **Port Ready** | **Bosh** | Light green | Free / Clear | `[BOSH] Port :8080 clear` |
| **Warning / Degraded** | **No gut** / **Xianlang** | Yellow | Not good / Danger | `[XIANLANG] Memory leak detected` |
| **Error / Failed** | **Kaka** | Red | Junk / Failure | `[KAKA] Unexpected stderr output` |
| **Crashed / Fatal** | **Pashangfong** | Dark red | Completely wrecked / broken | `[PASHANGFONG] Process exited (code 1)` |
| **Stopped / Dead** | **Det** | Muted red | Dead / Stopped | `[DET] Service stopped` |

---

## 4. Diagnostics, Logs, and the Error Curator

Vocabulary for handling `stdout` and `stderr` streams:

* **Kaka:** Noise, useless log spam, dirty stack traces.
* **Kore Xianlang (Core Error):** The distilled core of a failure extracted by the Curator.
* **Na sasa:** Unknown or unidentifiable error (*"Do not know"*).
* **Mera da log:** Inspect the log (`view logs`).
* **Pensa:** LLM agent inference or reasoning time (*to think*).
* **Kuxaku stderr:** Empty error stream (ideal state).

---

## 5. Actions & Controls (CLI & GUI Buttons)

Direct labels for buttons, flags, and terminal commands:

| Dev Action | Lang Belta | Suggested Command / Button |
| :--- | :--- | :--- |
| **Start / Run** | **Gowork** / **Du** | `du <service>` or `[ Du ]` button |
| **Stop / Kill** | **Kwit** | `kwit <service>` or `[ Kwit ]` button |
| **Restart** | **Mo du** | `mo-du <service>` *(do again)* |
| **Inspect / View Logs** | **Mera** | `mera <service>` *(look / observe)* |
| **Send to Agent** | **Pensa** | `pensa <error_id>` *(send to think)* |
| **Clear / Purge** | **Klin** | `klin logs` *(clean)* |
| **Confirm / OK** | **Sasa ke?** | Confirmation button / modal prompt |
| **Abort / Cancel** | **Nating** | Cancel action *(nothing / abort)* |
| **Settings / Config** | **Rula** | Service configuration |

---

## 6. UI Message Dictionary (i18n Strings)

Sample reference strings for localization tables (`src/i18n.ts`):

```json
{
  "hub.title": "ONE-SHIP // Control Hub",
  "status.nominal": "Kowlting gonya gut (All systems nominal)",
  "status.working": "Mi du work, bosmang (Executing tasks...)",
  "status.degraded": "Xianlang: Kapawu no gut (Subsystem alert)",
  "status.crashed": "Pashangfong! Kapawu det (Service crashed)",
  "action.start": "Gowork",
  "action.stop": "Kwit",
  "action.restart": "Mo du",
  "action.inspect": "Mera",
  "action.agent_consult": "Kowting Kopeng (Consult Agent)",
  "prompt.confirm": "Sasa ke? (Understood?)",
  "error.unknown": "Na sasa (Uncategorized failure)",
  "empty.services": "Kuxaku. Na kapawu du work (No active services)"
}
```

---

## 7. Daily Quote Bank (*The Expanse* & Belta)

Collection of 34 quotes from across the saga (the 9 novels by James S.A. Corey, novellas, and TV series), cataloged for **"Quote of the Day"** widgets, Hub statuses, tooltips, or diagnostic footers:

1. **"As cells are separate but also make a body, so a ship is made from a crew, and a people from their ships. Only one ship."**  
   *— Naomi Nagata (*Babylon's Ashes* / *One Ship*)*  
   *Dev Usage:* Hub welcome screen / ONE-SHIP Doctrine.

2. **"Kowlting gonya gut."** *(Everything's going to be fine)*  
   *— Belter proverb (Lang Belta)*  
   *Dev Usage:* Successful build, 100% passing test suite.

3. **"Mi du work, bosmang."** *(I'm on the job, boss)*  
   *— Common crew saying (Lang Belta)*  
   *Dev Usage:* Background task or LLM agent processing indicator.

4. **"If we cannot live together, we will die together. Sasa ke?"**  
   *— Camina Drummer (*Season 3*)*  
   *Dev Usage:* Cascading microservice failures or downed dependencies.

5. **"The stars are better off without us."**  
   *— Joe Miller (*Leviathan Wakes*)*  
   *Dev Usage:* Unrecoverable fatal crash, Kernel Panic, or global SIGKILL.

6. **"Doors and corners, kid. That's where they get you."**  
   *— Joe Miller (*Leviathan Wakes* / *Cibola Burn*)*  
   *Dev Usage:* Error Curator warning about edge cases and input sanitization.

7. **"Beltalowda, we are standing on the precipice of everything we ever fought for."**  
   *— Klaes Ashford (*Season 3*)*  
   *Dev Usage:* Production deployment or major version release.

8. **"You can't take the razorback, she's gone and gone."**  
   *— Julie Mao (*Leviathan Wakes*)*  
   *Dev Usage:* Orphan process abruptly terminated by the OOM Killer.

9. **"No law on Ceres, just cops."**  
   *— Joe Miller (*Leviathan Wakes*)*  
   *Dev Usage:* `--force` mode enabled, bypassing hooks or strict checks.

10. **"Tenye wa gut run, beratna."** *(Have a good run, brother)*  
    *— Belter farewell (Lang Belta)*  
    *Dev Usage:* Clean Daemon session shutdown or console exit.

11. **"I am that guy."**  
    *— Amos Burton (*Cibola Burn* / *Season 3*)*  
    *Dev Usage:* Privilege escalation (`pkexec` / root / admin override).

12. **"There was a button. I pushed it." — "Jesus Christ, that's really how you go through life, isn't it?"**  
    *— James Holden & Fred Johnson (*Leviathan Wakes*)*  
    *Dev Usage:* Uncontrolled manual event trigger or uncaught exception.

13. **"You don't get to choose who you are, only what you do."**  
    *— Amos Burton (*The Churn* / *Nemesis Games*)*  
    *Dev Usage:* Immutable state, pure functions, and deterministic architectures.

14. **"It reaches out, it reaches out, it reaches out..."**  
    *— The Investigator (*Abaddon's Gate* / *Cibola Burn*)*  
    *Dev Usage:* Retry loops and WebSocket / SSE reconnection probes.

15. **"Whatever you're doing, wherever you are, someone is suffering because of it."**  
    *— Chrisjen Avasarala (*Caliban's War*)*  
    *Dev Usage:* Breaking change or unintended API side-effect alert.

16. **"Don't stick your dick in it, James. It's fucked enough already."**  
    *— Chrisjen Avasarala (*Caliban's War*)*  
    *Dev Usage:* Critical warning before refactoring fragile legacy code.

17. **"If you tell a machine to make paperclips, eventually the universe is just paperclips."**  
    *— Prax Meng (*Caliban's War*)*  
    *Dev Usage:* Unbounded recursion, memory leaks, or infinite loops.

18. **"A good captain does not panic. A good captain delegates the panic."**  
    *— Klaes Ashford (*Abaddon's Gate*)*  
    *Dev Usage:* Asynchronous error dispatch to agents or work queues.

19. **"Inyalowda take the water, take the air, and now they want our mind."**  
    *— Anderson Dawes (*Season 1*)*  
    *Dev Usage:* Excessive RAM consumption or CPU hijacking alert from subprocesses.

20. **"We are the blood of the Belt. Our ancestors gave their lives to build this station."**  
    *— Anderson Dawes (*Leviathan Wakes*)*  
    *Dev Usage:* `git blame` / Inspecting foundational commits in a repository.

21. **"This is the room where it all comes together, or it all falls apart."**  
    *— Camina Drummer (*Season 3*)*  
    *Dev Usage:* Staging environment or end-to-end integration test runs.

22. **"You cannot stop what is coming. The future does not ask permission."**  
    *— Winston Duarte (*Persepolis Rising* / *Tiamat's Wrath*)*  
    *Dev Usage:* Irreversible database migrations or forced schema upgrades.

23. **"I didn't kill him because he was an enemy. I killed him because he was a problem."**  
    *— Amos Burton (*Nemesis Games*)*  
    *Dev Usage:* Forced `kill -9` on frozen or unresponsive processes.

24. **"Nothing is ever truly lost, it just changes form."**  
    *— Naomi Nagata (*Persepolis Rising*)*  
    *Dev Usage:* Garbage collection, cache persistence, or state snapshotting.

25. **"The universe has a rhythm. You fight it, you lose. You dance with it, you survive."**  
    *— Alex Kamal (*Cibola Burn*)*  
    *Dev Usage:* Reactive architecture, async runtimes, and event listeners.

26. **"No matter what happens, keep breathing. In the Belt, breath is everything."**  
    *— Belter proverb*  
    *Dev Usage:* Periodic heartbeat ping and Daemon health monitoring.

27. **"We make our own gravity out here."**  
    *— Camina Drummer (*Season 4*)*  
    *Dev Usage:* Greenfield project bootstrapping and custom toolchain setup.

28. **"The difference between a cage and a home is who holds the key."**  
    *— Clarissa Mao (*Nemesis Games*)*  
    *Dev Usage:* Authentication, API key management, and access control (IAM / RBAC).

29. **"If you don't look at the warning lights, they can't scare you."**  
    *— Amos Burton (*Babylon's Ashes*)*  
    *Dev Usage:* `--suppress-warnings` flags or ignored compiler/linter warnings.

30. **"A system that cannot adapt to failure is already dead."**  
    *— Michio Pa (*Babylon's Ashes*)*  
    *Dev Usage:* Fault-tolerant architecture, circuit breakers, and graceful degradation.

31. **"Da kuxaku na im enemy; im home."** *(The void is not our enemy; it is our home)*  
    *— Traditional Belter chant (Lang Belta)*  
    *Dev Usage:* Empty state or workspace ready to receive a project.

32. **"Things don't just fix themselves, Holden. People fix them."**  
    *— Naomi Nagata (*Caliban's War*)*  
    *Dev Usage:* Human-in-the-loop approval request when an agent is blocked.

33. **"Even when the signal is weak, you keep listening."**  
    *— OPA communications protocol (*Babylon's Ashes*)*  
    *Dev Usage:* Log streaming (`tail -f`) and continuous `stdout`/`stderr` capture.

34. **"Trust the math, not your gut. Your gut doesn't know orbital mechanics."**  
    *— Alex Kamal (*Leviathan Wakes*)*  
    *Dev Usage:* Strict typing, static analysis, and unit test assertions.
