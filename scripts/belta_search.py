#!/usr/bin/env python3
"""
Tekidok Lang Belta CLI Query Tool
Search Belter Creole words, idioms, sentences, grammar, and notes.

Usage:
    python3 scripts/belta_search.py <query>
    python3 scripts/belta_search.py --grammar
    python3 scripts/belta_search.py --notes
"""

import json
import sys
from pathlib import Path

DB_PATH = Path(__file__).parent.parent / "docs" / "tekidok_lang_belta.json"

def load_db():
    if not DB_PATH.exists():
        print(f"Error: Database not found at {DB_PATH}", file=sys.stderr)
        sys.exit(1)
    with open(DB_PATH, "r", encoding="utf-8") as f:
        return json.load(f)

def search(query):
    db = load_db()
    q = query.lower().strip()
    
    print("\n" + "=" * 55)
    print(f"  LANG BELTA SEARCH: \"{query}\"")
    print("=" * 55 + "\n")
    
    # 1. Words
    word_matches = []
    for w in db.get("words", []):
        belta = w.get("belta", "").lower()
        eng = w.get("english", "").lower()
        details = w.get("details", "").lower()
        alt = w.get("alt_spelling", "").lower()
        if q in belta or q in eng or q in details or q in alt:
            word_matches.append(w)
            
    # 2. Eyepatch & Cork (Idioms/Colloquialisms)
    epc_matches = []
    for w in db.get("eyepatch_and_cork", []):
        belta = w.get("belta", "").lower()
        eng = w.get("english", "").lower()
        details = w.get("details", "").lower()
        if q in belta or q in eng or q in details:
            epc_matches.append(w)
            
    # 3. Sentences
    sentence_matches = []
    for s in db.get("sentences", []):
        belta = s.get("belta", "").lower()
        eng = s.get("english", "").lower()
        details = s.get("details", "").lower()
        if q in belta or q in eng or q in details:
            sentence_matches.append(s)
            
    # 4. Grammar
    grammar_matches = []
    for g in db.get("grammar", []):
        topic = g.get("topic", "").lower()
        rule = g.get("rule", "").lower()
        if q in topic or q in rule:
            grammar_matches.append(g)

    # Output Words
    if word_matches:
        print(f"[+] Words Found ({len(word_matches)}):")
        for w in word_matches[:15]:
            pos = f" ({w['pos']})" if w.get('pos') else ""
            alt = f" [alt: {w['alt_spelling']}]" if w.get('alt_spelling') else ""
            print(f"  * {w['belta']}{alt}{pos}: {w['english']}")
            if w.get('details'):
                print(f"    Details: {w['details']}")
        if len(word_matches) > 15:
            print(f"    ... and {len(word_matches) - 15} more matches.")
        print()

    # Output Eyepatch & Cork
    if epc_matches:
        print(f"[+] Idioms & Colloquialisms ({len(epc_matches)}):")
        for w in epc_matches[:10]:
            print(f"  * {w['belta']}: {w['english']}")
            if w.get('details'):
                print(f"    Details: {w['details']}")
        if len(epc_matches) > 10:
            print(f"    ... and {len(epc_matches) - 10} more matches.")
        print()

    # Output Sentences
    if sentence_matches:
        print(f"[+] Example Sentences ({len(sentence_matches)}):")
        for s in sentence_matches[:8]:
            print(f"  * \"{s['belta']}\"")
            print(f"    -> \"{s['english']}\"")
            if s.get('details'):
                print(f"    Context: {s['details']}")
        if len(sentence_matches) > 8:
            print(f"    ... and {len(sentence_matches) - 8} more matches.")
        print()

    # Output Grammar
    if grammar_matches:
        print(f"[+] Grammar Rules ({len(grammar_matches)}):")
        for g in grammar_matches:
            topic = f"[{g['topic']}] " if g.get('topic') else ""
            print(f"  * {topic}{g['rule']}")
        print()

    if not (word_matches or epc_matches or sentence_matches or grammar_matches):
        print(f"No matches found for '{query}'.")

def show_grammar():
    db = load_db()
    print("\n" + "=" * 55)
    print("  LANG BELTA GRAMMAR RULES")
    print("=" * 55 + "\n")
    for g in db.get("grammar", []):
        topic = f"[{g['topic']}] " if g.get('topic') else ""
        print(f"* {topic}{g['rule']}")
        if g.get('source'):
            print(f"  Source: {g['source']}")
        print()

def show_notes():
    db = load_db()
    print("\n" + "=" * 55)
    print("  LANG BELTA CULTURAL & LINGUISTIC NOTES")
    print("=" * 55 + "\n")
    for n in db.get("notes", []):
        print(f"* {n['note']}")
        if n.get('source'):
            print(f"  Source: {n['source']}")
        print()

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(0)
        
    arg = sys.argv[1]
    if arg == "--grammar":
        show_grammar()
    elif arg == "--notes":
        show_notes()
    else:
        search(" ".join(sys.argv[1:]))
