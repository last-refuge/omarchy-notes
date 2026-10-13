#!/usr/bin/env python3
"""Opt-in live Wayland test. Replaces clipboard with test data; never reads its previous contents.
Native input (if any) is guarded by owned PID/address/app ID/workspace before and after each key.
Uses scratch libraries and closes every launched process. Requires compiled binaries and wl-clipboard.
"""
import subprocess, tempfile, json, select
from pathlib import Path
root=Path(tempfile.mkdtemp(prefix='notes-clipboard-check-'))
probe=Path(__file__).resolve().parents[1]/'target/debug/examples/clipboard-probe'
fragment={'schema':1,'blocks':[{'id':'clipboard-check','type':'paragraph','spans':[{'text':'A bright idea','marks':['bold']}]}]}
(root/'fragment.json').write_text(json.dumps(fragment));(root/'plain.txt').write_text('A bright idea');(root/'rich.html').write_text('<p>A <b>bright</b> idea</p>')
children=[]
def offer(args):
 p=subprocess.Popen([str(probe),'offer',*args],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);children.append(p)
 assert select.select([p.stdout],[],[],10)[0], 'Clipboard owner did not become ready'
 assert p.stdout.readline().strip()=='READY', p.stderr.read();return p
try:
 p=offer(['application/x-omarchy-notes-fragment+json',str(root/'fragment.json'),'text/html',str(root/'rich.html'),'text/plain;charset=utf-8',str(root/'plain.txt')])
 types=subprocess.check_output(['wl-paste','--list-types'],timeout=8,text=True)
 for mime,file in [('application/x-omarchy-notes-fragment+json','fragment.json'),('text/html','rich.html'),('text/plain;charset=utf-8','plain.txt')]:
  assert mime in types;assert subprocess.check_output(['wl-paste','--no-newline','--type',mime],timeout=8)==(root/file).read_bytes()
 p.stdin.write('self\n');p.stdin.flush();assert select.select([p.stdout],[],[],10)[0];assert p.stdout.readline().startswith('application/x-omarchy-notes-fragment+json')
 mime=subprocess.check_output([str(probe),'paste',str(root/'pasted')],timeout=10,text=True).strip();assert mime=='application/x-omarchy-notes-fragment+json';assert (root/'pasted').read_bytes()==(root/'fragment.json').read_bytes()
 mime=subprocess.check_output([str(probe),'plain',str(root/'pasted')],timeout=10,text=True).strip();assert mime=='text/plain;charset=utf-8';assert (root/'pasted').read_text()=='A bright idea'
 p2=offer(['text/html',str(root/'rich.html'),'text/plain;charset=utf-8',str(root/'plain.txt')]);mime=subprocess.check_output([str(probe),'paste',str(root/'pasted')],timeout=10,text=True).strip();assert mime=='text/html'
 print(json.dumps({'result':'PASS','checks':['multi-MIME owner','wl-paste exact bytes','own clipboard avoids deadlock','cross-process rich preference','plain-text preference','HTML preference'], 'scratch':str(root)}))
finally:
 for p in children:
  if p.poll() is None:
   p.stdin.close()
   try:p.wait(timeout=5)
   except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)
