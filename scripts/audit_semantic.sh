#!/usr/bin/env bash
set -euo pipefail

# Client CLI bridge for auditing the live ONE-SHIP semantic index & knowledge graph
# via the active daemon on 127.0.0.1 (zero duplicate ONNX / LanceDB instances).

DAEMON_PORT="${ONESHIP_DAEMON_PORT:-8765}"
PROJECT_ID=""
REINDEX="false"
CANDIDATE_MIN=60
LIMIT=5
RAW_JSON="false"
QUERIES=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --project-id|-p)
      PROJECT_ID="$2"
      shift 2
      ;;
    --reindex|-r)
      REINDEX="true"
      shift
      ;;
    --candidate-min|-c)
      CANDIDATE_MIN="$2"
      shift 2
      ;;
    --limit|-l)
      LIMIT="$2"
      shift 2
      ;;
    --query|-q)
      QUERIES+=("$2")
      shift 2
      ;;
    --raw)
      RAW_JSON="true"
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 --project-id <id> [--reindex] [--candidate-min 60] [--limit 5] [--query '...'] [--raw]" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$PROJECT_ID" ]]; then
  echo "Error: --project-id <id> is required." >&2
  exit 1
fi

PAYLOAD="$(python3 - "$PROJECT_ID" "$REINDEX" "$CANDIDATE_MIN" "$LIMIT" "${QUERIES[@]}" <<'PY'
import json, sys
project_id = int(sys.argv[1])
reindex = sys.argv[2].lower() == "true"
candidate_min = int(sys.argv[3])
limit = int(sys.argv[4])
queries = sys.argv[5:]
print(json.dumps({
    "projectId": project_id,
    "reindex": reindex,
    "candidateMinScorePercent": candidate_min,
    "limit": limit,
    "queries": queries,
}))
PY
)"

RESPONSE="$(curl -sS -X POST "http://127.0.0.1:${DAEMON_PORT}/api/v1/audit/semantic" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD")"

if [[ "$RAW_JSON" == "true" ]]; then
  echo "$RESPONSE" | python3 -m json.tool
  exit 0
fi

python3 - "$RESPONSE" <<'PY'
import json, sys

raw = sys.argv[1]
try:
    data = json.loads(raw)
except Exception as exc:
    print(f"Failed to parse daemon response: {exc}\nRaw: {raw}", file=sys.stderr)
    sys.exit(1)

if data.get("status") != "ok":
    print(f"Audit Error: {data.get('message', raw)}", file=sys.stderr)
    sys.exit(1)

res = data["result"]
st = res["status"]
print("================================================================================")
print(f" ONE-SHIP SEMANTIC & KNOWLEDGE GRAPH AUDIT — Project ID: {st['projectId']}")
print("================================================================================")
print(f" Total Chunks : {st['totalRecordsCount']} (Code: {st['codeChunksCount']}, Specs: {st['specChunksCount']}, Entities: {st['entityChunksCount']})")
print(f" Graph Edges  : {st['graphEdgesCount']}  |  Auto-Sync: {'ON' if st['autoIndexEnabled'] else 'OFF'}  |  Active Min Score: {st['minScorePercent']}%")
if st.get("lastDurationMs") is not None:
    print(f" Last Run     : {st['lastDurationMs']} ms")

print("\n--- Knowledge Graph Breakdown (by Edge Type & Origin) ---")
groups = res.get("graphAudit", {}).get("byTypeAndOrigin", [])
if not groups:
    print("  (No edges recorded)")
for g in groups:
    print(f"  - {g['edgeType']:10s} [{g['origin']:12s}] : {g['count']:4d} edges  (weight min={g['minWeight']}, avg={g['avgWeight']}, max={g['maxWeight']})")

print("\n--- Sample Active Knowledge Graph Edges ---")
for e in res.get("graphAudit", {}).get("sampleEdges", [])[:15]:
    src = f"{e['source']['type']}:{e['source']['title']}"
    tgt = f"{e['target']['type']}:{e['target']['title']}"
    print(f"  [{e['origin']:12s} | {e['scorePercent']:5.1f}%] {src} --({e['edgeType']})--> {tgt}")

cand = res.get("crossDomainCandidates", {})
pairs = cand.get("topPairs", [])
print(f"\n--- Cross-Domain Vector Similarity Candidates (>= {cand.get('candidateMinScorePercent')}%, active cutoff = {cand.get('activeMinScorePercent')}%) ---")
if not pairs:
    print("  (No cross-domain candidates above floor)")
for p in pairs[:20]:
    mark = "PASS" if p["aboveActiveThreshold"] else "DROP"
    print(f"  [{mark} {p['scorePercent']:5.1f}%] (journal) {p['journalTitle']}  <==>  (specs) {p['specTitle']} [{p['specFile']}:L{p['specLineStart']}]")

evals = res.get("queryEvaluations", [])
if evals:
    print("\n--- Query Evaluations ---")
    for ev in evals:
        print(f"\n  Query: \"{ev['query']}\" ({ev['resultsCount']} hits)")
        for idx, h in enumerate(ev.get("results", []), 1):
            loc = f"{h.get('filePath') or ''}:L{h.get('lineStart') or 0}-{h.get('lineEnd') or 0}"
            print(f"    {idx}. [{h['scorePercent']:5.1f}%] [{h['entityType'].upper()}] {h['title']} ({loc})")
            for r in h.get("relations", []):
                arrow = "-->" if r["direction"] == "outgoing" else "<--"
                print(f"         Graph {arrow} ({r['edgeType']}) [{r['peerEntityType']}] {r['peerTitle']} ({r.get('peerFilePath') or ''}:L{r.get('peerLineStart') or 0})")
PY
