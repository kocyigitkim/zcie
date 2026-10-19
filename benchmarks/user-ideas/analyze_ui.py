#!/usr/bin/env python3
"""User-ideas capsule check without a model (ADR-017, D-055 and D-065).

Usage: analyze_ui.py <questions.json> [project folder]
For each question: the top 10 results of the installed daemon and the capsule of the installed
context hook (~/.zcie/hooks/inject-context.sh). Prints if each expected item is in them and the
capsule size (characters / 4). The project folder (default: the current folder) must have
.claude/zcie_id. It only reads: the hook gets a new session ID for each question.
"""
import json, os, subprocess, sys, unicodedata, urllib.request
def fold(t):
    t=unicodedata.normalize("NFKD",t.lower()); return "".join(c for c in t if not unicodedata.combining(c)).replace("ı","i")
key=open(os.path.expanduser('~/.zcie/config/claude-code.key')).read().strip()
env={k:v for k,v in os.environ.items() if k not in ('ZCIE_WORKSPACE','ZCIE_API_KEY_FILE')}
env['ZCIE_API_KEY_FILE']=os.path.expanduser('~/.zcie/config/claude-code.key')
qs=json.load(open(sys.argv[1])); cwd=sys.argv[2] if len(sys.argv)>2 else os.getcwd()
pid=open(os.path.join(cwd,'.claude','zcie_id')).read().strip()
def missing(text, q): return [(i if isinstance(i,str) else i[0]) for i in q["expect"] if not any(fold(o) in fold(text) for o in (i if isinstance(i,list) else [i]))]
cap_ok=top_ok=0
for q in qs:
    req=urllib.request.Request('http://127.0.0.1:47831/v1/query', data=json.dumps({"workspace":pid,"question":q["prompt"],"token_budget":4000,"max_results":10,"cross_project":True}).encode(), method='POST')
    req.add_header('Authorization','Bearer '+key); req.add_header('Content-Type','application/json')
    res=json.loads(urllib.request.urlopen(req,timeout=60).read())
    top=" ".join(r["title"]+" "+(r.get("snippet") or "") for r in res["results"])
    out=subprocess.run(["sh",os.path.expanduser("~/.zcie/hooks/inject-context.sh")],input=json.dumps({"session_id":f"an-{q['stage']}-{os.getpid()}","cwd":cwd,"prompt":q["prompt"]}),capture_output=True,text=True,env=env).stdout
    mt=missing(top,q); mc=missing(out,q); top_ok+=not mt; cap_ok+=not mc
    print(f"Q{q['stage']:02d} class={res['class']:<12} cov={res.get('match_coverage')} top10_missing={mt} capsule_missing={mc} capsule_tokens~{len(out)//4}")
print(f"in top 10: {top_ok}/{len(qs)}  in capsule: {cap_ok}/{len(qs)}")
