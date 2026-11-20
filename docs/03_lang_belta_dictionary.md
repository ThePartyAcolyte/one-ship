# Lang Belta Dictionary: Software & Cockpit Lexicon
**Ecosistema:** ONE-SHIP (`one-ship`)  
**Contexto:** Localización i18n, terminología de sistema, diagnóstico y mensajes de interfaz inspirados en *Lang Belta* (Belter Creole de *The Expanse*).

> [!NOTE]
> **Status: DEFERRED UNTIL PACKAGING (Post-Completion Translation Pass)**
> Active development of ONE-SHIP's codebase, UI, and technical specifications is conducted strictly in English (`AGENTS.md` §1.2). This dictionary and the Cinturonero translation protocol (Specs 07 & 14) are parked until the application reaches functional completion and packaging, at which point localization will be executed as a dedicated, separate process.

---

## 1. Filosofía de Base: Da Doctrina de ONE-SHIP

En la cultura cinturonera (*Beltalowda*), el principio fundamental de supervivencia es la **Doctrina de la Única Nave** (*Doctrine of the One Ship*):

> *"As cells are separate but also make a body, so a ship is made from a crew, and a people from their ships. Only one ship."*  
> — Naomi Nagata / Michio Pa

En el contexto de **ONE-SHIP**:
* El workspace y su ecosistema no son procesos aislados: son una sola nave (**ONE-SHIP**).
* La interfaz es el puente de mando (**Stashang** / **Cockpit**).
* El daemon supervisor es el **Bosmang**.
* Los agentes de IA y herramientas son la tripulación (**Kopenglowda**).
* Un bug o servicio caído no es un evento aislado, es una despresurización que afecta a la nave entera.

---

## 2. Mapeo de Entidades (Ubiquitous Language)

Mapeo directo de los conceptos definidos en [`01_ubiquitous_language.md`](01_ubiquitous_language.md) hacia *Lang Belta*:

| Término Original (DDD) | Lang Belta | Pronunciación / Etimología | Rol en el Sistema |
| :--- | :--- | :--- | :--- |
| **Hub** | **Kapawu** / **Stashang** | *ka-PA-wu* (del malayo *kapal*, nave) / *sta-SHANG* (estación) | La ventana principal GUI. El puente de mando del proyecto. |
| **Daemon** | **Bosmang** | *BOS-mang* (del inglés *boss-man*) | El proceso Rust en segundo plano que vigila y orquesta todo. |
| **Tray Hub** | **Lek Stashang** | *lek* (holandés/afrikáans *klein*, pequeño) + *stashang* | El menú compacto en la barra de estado del sistema. |
| **Tracker Config** | **Rula** / **Plan** | *RU-la* (reglas/configuración) | Archivo de configuración local (`rula.toml`). |
| **Service / Subprocess** | **Kapawulowda** | *ka-pa-wu-LOW-da* (sufijo plural colectivo) | Cada servicio o comando individual levantado por el Daemon. |
| **Error Curator** | **Kaka-Tresh** | *KA-ka* (basura/error) + *tresh* (del inglés *trash*) | Subsistema que filtra `stderr`, elimina ruido y aísla la falla. |
| **Core Error** | **Kore Xianlang** | *kore* (núcleo) + *SHAN-lang* (chino 险浪, peligroso/alerta) | El error limpio y sintetizado, listo para el LLM. |
| **Agent Link** | **Kopeng-Link** | *KO-peng* (amigo/compañero/tripulante) | Conexión directa hacia el agente (`prompt_agent`). |
| **Developer / User** | **Pampa** / **Beratna** | *PAM-pa* (veterano/anciano) / *be-RAT-na* (hermano) | El desarrollador a los mandos. |

---

## 3. Estados del Sistema (Telemetry & Health Checks)

Términos estándar para badges, indicadores de salud de servicios y salidas de consola:

| Estado Dev | Lang Belta | Tono / UI | Significado Literal | Mensaje en Consola / UI |
| :--- | :--- | :--- | :--- | :--- |
| **Healthy / OK** | **Gut** | Verde | Bueno / Nominal | `[GUT] Kapawu operational` |
| **All nominal** | **Kowlting gut** | Verde | Todo está bien | `Kowlting gonya gut` (Todo irá bien) |
| **Running** | **Du work** | Azul / Cyan | Haciendo trabajo | `[DU WORK] Proceso activo en PID 4120` |
| **Idle / Standby** | **Paxon** | Gris / Cyan | Paz / Tranquilidad | `[PAXON] Esperando comandos...` |
| **Empty / Unset** | **Kuxaku** | Gris | Vacío | `[KUXAKU] Ningún servicio asignado` |
| **Port Ready** | **Bosh** | Verde claro | Libre / Despejado | `[BOSH] Puerto :8080 libre` |
| **Warning / Degraded** | **No gut** / **Xianlang** | Amarillo | No bueno / Peligro | `[XIANLANG] Fuga de memoria detectada` |
| **Error / Failed** | **Kaka** | Rojo | Basura / Fallo | `[KAKA] Salida inesperada en stderr` |
| **Crashed / Fatal** | **Pashangfong** | Rojo oscuro | Completamente jodido / roto | `[PASHANGFONG] Proceso terminado (code 1)` |
| **Stopped / Dead** | **Det** | Rojo apagado | Muerto / Detenido | `[DET] Servicio apagado` |

---

## 4. Diagnóstico, Logs y el Error Curator

Vocabulario para el tratamiento de flujos `stdout` y `stderr`:

* **Kaka:** Ruido, spam de logs innecesario, stack trace sucio.
* **Kore Xianlang (Core Error):** El núcleo de la falla una vez extraído por el Curator.
* **Na sasa:** Error desconocido o no identificable (*"No se sabe"*).
* **Mera da log:** Inspeccionar el log (`view logs`).
* **Pensa:** Tiempo de inferencia o análisis del agente LLM (*pensar / razonar*).
* **Kuxaku stderr:** Flujo de errores vacío (estado ideal).

---

## 5. Acciones y Controles (CLI & GUI Buttons)

Nombres directos para botones, flags y comandos de terminal:

| Acción Dev | Lang Belta | Comando / Botón sugerido |
| :--- | :--- | :--- |
| **Start / Run** | **Gowork** / **Du** | `du <service>` o botón `[ Du ]` |
| **Stop / Kill** | **Kwit** | `kwit <service>` o botón `[ Kwit ]` |
| **Restart** | **Mo du** | `mo-du <service>` *(hacer de nuevo)* |
| **Inspect / View Logs** | **Mera** | `mera <service>` *(mirar / observar)* |
| **Send to Agent** | **Pensa** | `pensa <error_id>` *(mandar a pensar)* |
| **Clear / Purge** | **Klin** | `klin logs` *(limpiar)* |
| **Confirm / OK** | **Sasa ke?** | Botón de confirmación / Modal prompt |
| **Abort / Cancel** | **Nating** | Cancelar acción *(nada / cancelar)* |
| **Settings / Config** | **Ajustes** / **Rula** | Configuración de servicios |

---

## 6. Diccionario de Mensajes de Interfaz (i18n UI Strings)

Strings listos para implementar en archivos de localización (ej. `src/locales/belta.json`):

```json
{
  "hub.title": "ONE-SHIP // Hub de Control",
  "status.nominal": "Kowlting gonya gut (Todo en orden)",
  "status.working": "Mi du work, bosmang (Ejecutando tareas...)",
  "status.degraded": "Xianlang: Kapawu no gut (Alerta en subsistema)",
  "status.crashed": "Pashangfong! Kapawu det (Servicio colapsado)",
  "action.start": "Gowork",
  "action.stop": "Kwit",
  "action.restart": "Mo du",
  "action.inspect": "Mera",
  "action.agent_consult": "Kowting Kopeng (Consultar al Agente)",
  "prompt.confirm": "Sasa ke? (¿Entendido?)",
  "error.unknown": "Na sasa (Falla no categorizada)",
  "empty.services": "Kuxaku. Na kapawu du work (Sin servicios activos)"
}
```

---

## 7. Banco de Frases Diarias (Quotes de *The Expanse* & Belta)

Colección de 34 citas de la saga completa (los 9 libros de James S.A. Corey, novelas cortas y la serie de TV), traducidas y catalogadas para widgets de **"Frase del Día"**, estados del Hub, tooltips o pie de página del diagnóstico:

1. **"As cells are separate but also make a body, so a ship is made from a crew, and a people from their ships. Only one ship."**  
   *— Naomi Nagata (*Babylon's Ashes* / *One Ship*)*  
   *Uso Dev:* Pantalla de bienvenida del Hub / Doctrina de ONE-SHIP.

2. **"Kowlting gonya gut."** *(Todo va a salir bien)*  
   *— Proverbio del Cinturón (Lang Belta)*  
   *Uso Dev:* Build exitoso, suites de tests completadas al 100% en verde.

3. **"Mi du work, bosmang."** *(Estoy en el trabajo, jefe)*  
   *— Dicho común de tripulación (Lang Belta)*  
   *Uso Dev:* Indicador de tareas en segundo plano o agentes LLM procesando.

4. **"If we cannot live together, we will die together. Sasa ke?"**  
   *— Camina Drummer (*Temporada 3*)*  
   *Uso Dev:* Fallas en cascada de microservicios o dependencias caídas.

5. **"The stars are better off without us."**  
   *— Joe Miller (*Leviathan Wakes*)*  
   *Uso Dev:* Crash fatal no recuperable, Kernel Panic o SIGKILL global.

6. **"Doors and corners, kid. That's where they get you."**  
   *— Joe Miller (*Leviathan Wakes* / *Cibola Burn*)*  
   *Uso Dev:* Advertencia en Error Curator sobre edge cases y sanitización de inputs.

7. **"Beltalowda, we are standing on the precipice of everything we ever fought for."**  
   *— Klaes Ashford (*Temporada 3*)*  
   *Uso Dev:* Deploy a producción o release exitoso de una versión mayor.

8. **"You can't take the razorback, she's gone and gone."**  
   *— Julie Mao (*Leviathan Wakes*)*  
   *Uso Dev:* Proceso huérfano terminado abruptamente por el OOM Killer.

9. **"No law on Ceres, just cops."**  
   *— Joe Miller (*Leviathan Wakes*)*  
   *Uso Dev:* Modo `--force` activado, bypass de hooks o validaciones estrictas.

10. **"Tenye wa gut run, beratna."** *(Que tengas una buena travesía, hermano)*  
    *— Despedida cinturonera (Lang Belta)*  
    *Uso Dev:* Cierre limpio de sesión del Daemon o apagado de la consola.

11. **"I am that guy."**  
    *— Amos Burton (*Cibola Burn* / *Temporada 3*)*  
    *Uso Dev:* Elevación de privilegios (sudo / root / admin override).

12. **"There was a button. I pushed it." — "Jesus Christ, that's really how you go through life, isn't it?"**  
    *— James Holden & Fred Johnson (*Leviathan Wakes*)*  
    *Uso Dev:* Disparo manual de eventos no controlados o excepciones no capturadas.

13. **"You don't get to choose who you are, only what you do."**  
    *— Amos Burton (*The Churn* / *Nemesis Games*)*  
    *Uso Dev:* Estado inmutable, funciones puras y determinismo en arquitecturas.

14. **"It reaches out, it reaches out, it reaches out..."**  
    *— The Investigator (*Abaddon's Gate* / *Cibola Burn*)*  
    *Uso Dev:* Bucles de reintento (retry loops) y reconexión de WebSockets.

15. **"Whatever you’re doing, wherever you are, someone is suffering because of it."**  
    *— Chrisjen Avasarala (*Caliban's War*)*  
    *Uso Dev:* Alerta de breaking changes o efectos secundarios no deseados en APIs.

16. **"Don't stick your dick in it, James. It's fucked enough already."**  
    *— Chrisjen Avasarala (*Caliban's War*)*  
    *Uso Dev:* Advertencia crítica al intentar refactorizar código legacy altamente frágil.

17. **"If you tell a machine to make paperclips, eventually the universe is just paperclips."**  
    *— Prax Meng (*Caliban's War*)*  
    *Uso Dev:* Recursión descontrolada, memory leaks o loops infinitos.

18. **"A good captain does not panic. A good captain delegates the panic."**  
    *— Klaes Ashford (*Abaddon's Gate*)*  
    *Uso Dev:* Despacho asíncrono de errores a agentes o colas de trabajo (queues).

19. **"Inyalowda take the water, take the air, and now they want our mind."**  
    *— Anderson Dawes (*Temporada 1*)*  
    *Uso Dev:* Alerta por consumo excesivo de RAM o CPU hijacking por subprocesos.

20. **"We are the blood of the Belt. Our ancestors gave their lives to build this station."**  
    *— Anderson Dawes (*Leviathan Wakes*)*  
    *Uso Dev:* `git blame` / Inspección de commits fundacionales de un repositorio.

21. **"This is the room where it all comes together, or it all falls apart."**  
    *— Camina Drummer (*Temporada 3*)*  
    *Uso Dev:* Entorno de Staging o ejecución de tests de integración end-to-end.

22. **"You cannot stop what is coming. The future does not ask permission."**  
    *— Winston Duarte (*Persepolis Rising* / *Tiamat's Wrath*)*  
    *Uso Dev:* Migraciones de base de datos irreversibles o actualizaciones forzadas.

23. **"I didn't kill him because he was an enemy. I killed him because he was a problem."**  
    *— Amos Burton (*Nemesis Games*)*  
    *Uso Dev:* Comando `kill -9` forzado sobre procesos congelados o no responsivos.

24. **"Nothing is ever truly lost, it just changes form."**  
    *— Naomi Nagata (*Persepolis Rising*)*  
    *Uso Dev:* Garbage collection, persistencia en caché o snapshotting de estados.

25. **"The universe has a rhythm. You fight it, you lose. You dance with it, you survive."**  
    *— Alex Kamal (*Cibola Burn*)*  
    *Uso Dev:* Arquitectura reactiva, programación asíncrona y event listeners.

26. **"No matter what happens, keep breathing. In the Belt, breath is everything."**  
    *— Proverbio cinturonero*  
    *Uso Dev:* Heartbeat ping periódico y monitoreo de salud del Daemon.

27. **"We make our own gravity out here."**  
    *— Camina Drummer (*Temporada 4*)*  
    *Uso Dev:* Bootstrapping de nuevos proyectos y configuración de toolchains personalizadas.

28. **"The difference between a cage and a home is who holds the key."**  
    *— Clarissa Mao (*Nemesis Games*)*  
    *Uso Dev:* Autenticación, gestión de API Keys y permisos de acceso (IAM / RBAC).

29. **"If you don't look at the warning lights, they can't scare you."**  
    *— Amos Burton (*Babylon's Ashes*)*  
    *Uso Dev:* Flags `--suppress-warnings` o ignorar warnings del compilador/linter.

30. **"A system that cannot adapt to failure is already dead."**  
    *— Michio Pa (*Babylon's Ashes*)*  
    *Uso Dev:* Arquitectura tolerante a fallos, circuit breakers y degradación elegante.

31. **"Da kuxaku na im enemy; im home."** *(El vacío no es nuestro enemigo; es nuestro hogar)*  
    *— Canto tradicional cinturonero (Lang Belta)*  
    *Uso Dev:* Estado vacío (*empty state*) o workspace listo para recibir un proyecto.

32. **"Things don't just fix themselves, Holden. People fix them."**  
    *— Naomi Nagata (*Caliban's War*)*  
    *Uso Dev:* Solicitud de intervención manual (Human-in-the-loop) cuando el agente falla.

33. **"Even when the signal is weak, you keep listening."**  
    *— Protocolo de comunicaciones de la OPA (*Babylon's Ashes*)*  
    *Uso Dev:* Streaming de logs (`tail -f`), captura continua de `stdout`/`stderr`.

34. **"Trust the math, not your gut. Your gut doesn't know orbital mechanics."**  
    *— Alex Kamal (*Leviathan Wakes*)*  
    *Uso Dev:* Tipado estricto, análisis estático y aserciones en pruebas unitarias.

