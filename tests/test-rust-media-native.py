#!/usr/bin/env python3
"""Opt-in live Wayland test. Replaces clipboard with test data; never reads its previous contents.
Native input (if any) is guarded by owned PID/address/app ID/workspace before and after each key.
Uses scratch libraries and closes every launched process. Requires compiled binaries and wl-clipboard.
"""
import json, os, sqlite3, subprocess, tempfile, time, select, hashlib
from pathlib import Path
from rust_native_test import InstanceSocket
root=Path(tempfile.mkdtemp(prefix='notes-media-native-'))
os.environ['XDG_CONFIG_HOME']=str(root/'config')
repo=Path(__file__).resolve().parents[1];binary=repo/'target/release/omarchy-notes-preview';probe=repo/'target/debug/examples/clipboard-probe'
children=[];owners=[];instance_sockets=[];active=None;address=None
fixture=repo/'tests/fixtures/qt-v1/blobs'
png=fixture/'bd/bdec1de53e3c856d6973c7324a5d719d6f6169e01a6f75ae3b4453f419d625e2';pdf=root/'Release notes.pdf';pdf.write_bytes((fixture/'d7/d7449961543b028f75a5778a6cbcfb8d3b7ea2dfe66da1efc60d9747f4fc4803').read_bytes())
(root/'rich.html').write_text('<p>A <b>bright</b> idea, with <i>room to grow</i>.</p><ul><li>Keep the details</li></ul><p>Picture this:</p>')
(root/'plain.txt').write_text('A bright idea, with room to grow.\nKeep the details\nPicture this:')
(root/'files.txt').write_text(pdf.as_uri()+'\r\n')
def read_json(*args):return json.loads(subprocess.check_output(['hyprctl',*args],timeout=3))
def guard():
 assert active and active.poll() is None,'Owned process exited'
 w=read_json('activewindow','-j');ws=read_json('activeworkspace','-j')
 assert w.get('pid')==active.pid and w.get('address')==address and w.get('class')=='org.omarchy.Notes.Preview','Refusing input: test window lost focus'
 assert w['workspace']['id']==ws['id'],'Refusing input: workspace changed'
def launch(name):
 global active,address
 with (root/f'{name}.log').open('a') as log:active=subprocess.Popen([str(binary),'--reference','--data-dir',str(root/name)],stdout=log,stderr=log)
 children.append(active);deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  own=[w for w in read_json('clients','-j') if w.get('pid')==active.pid and w.get('class')=='org.omarchy.Notes.Preview']
  if own:address=own[0]['address'];guard();instance_sockets.append(InstanceSocket(root/name,active));time.sleep(.3);return
  assert active.poll() is None,(root/f'{name}.log').read_text();time.sleep(.1)
 raise AssertionError('Window did not appear')
def key(name,ctrl=False,shift=False):
 assert name in ['n','v','a','c','s','q','Return','Right','BackSpace','z']
 args=['wtype'];args+=['-M','ctrl'] if ctrl else [];args+=['-M','shift'] if shift else [];args+=['-k',name];args+=['-m','shift'] if shift else [];args+=['-m','ctrl'] if ctrl else []
 guard();subprocess.run(args,check=True,timeout=3);guard()
def text(value):
 for ch in value:
  assert ch.isprintable();guard();subprocess.run(['wtype','--',ch],check=True,timeout=3);guard()
def offer(values):
 args=[str(probe),'offer']
 for mime,path in values:args += [mime,str(path)]
 p=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);owners.append(p)
 assert select.select([p.stdout],[],[],10)[0];assert p.stdout.readline().strip()=='READY',p.stderr.read()
def notes(name):
 with sqlite3.connect(f'file:{root/name}/library.sqlite3?mode=ro',uri=True) as db:return [(i,t,json.loads(b)) for i,t,b in db.execute('SELECT id,plain_text,body FROM notes ORDER BY created_at')]
def spans(body):
 def blocks(bs):
  for b in bs:
   yield from b.get('spans',[])
   for c in b.get('cells',[]):yield from blocks(c['blocks'])
 return list(blocks(body['blocks']))
def wait(name,predicate):
 deadline=time.monotonic()+12
 while time.monotonic()<deadline:
  rows=notes(name)
  if predicate(rows):return rows
  assert active.poll() is None,(root/f'{name}.log').read_text();time.sleep(.05)
 raise AssertionError(f'Expected durable state missing in {name}: {notes(name)}')
def media(rows):return [s for _,_,body in rows for s in spans(body) if 'image' in s or 'attachment' in s]
try:
 launch('source');key('n',True);time.sleep(.3);text('A note with a little more');key('Return')
 offer([('text/html',root/'rich.html'),('text/plain;charset=utf-8',root/'plain.txt')]);key('v',True)
 rows=wait('source',lambda rows:rows and 'Picture this:' in rows[0][1]);assert any(s.get('text')=='bright' and 'bold' in s.get('marks',[]) for s in spans(rows[0][2]))
 key('Return');offer([('image/png',png)]);key('v',True);wait('source',lambda rows:any('image'in s for s in media(rows)))
 key('Return');offer([('text/uri-list',root/'files.txt')]);key('v',True);source=wait('source',lambda rows:len(media(rows))==2)
 key('a',True);key('c',True)
 deadline=time.monotonic()+12;bundle=None
 while time.monotonic()<deadline:
  types=subprocess.check_output(['wl-paste','--list-types'],timeout=5,text=True)
  if 'application/x-omarchy-notes-fragment+json' in types:
   bundle=json.loads(subprocess.check_output(['wl-paste','--no-newline','--type','application/x-omarchy-notes-fragment+json'],timeout=5));break
  time.sleep(.05)
 assert bundle and len(bundle['omarchy_clipboard']['assets'])==2
 # Second real application process consumes the first app's rich clipboard and its asset bytes.
 launch('destination');key('n',True);time.sleep(.3);key('v',True)
 dest=wait('destination',lambda rows:len(media(rows))==2)
 assert 'bright' in dest[0][1];assert any('bold'in s.get('marks',[]) for s in spans(dest[0][2]))
 source_id=next(s['attachment'] for s in media(source) if 'attachment'in s);dest_id=next(s['attachment'] for s in media(dest) if 'attachment'in s);assert source_id!=dest_id
 for asset in bundle['omarchy_clipboard']['assets']:
  h=asset['hash'];a=(root/'source/blobs'/h[:2]/h).read_bytes();b=(root/'destination/blobs'/h[:2]/h).read_bytes();assert a==b and hashlib.sha256(b).hexdigest()==h
 key('n',True);time.sleep(.3);key('v',True,True);plain=wait('destination',lambda rows:len(rows)==2 and 'Picture this:' in rows[1][1]);assert len(media(plain))==2
 assert all(not s.get('marks') for s in spans(plain[1][2]))
 active.kill();active.wait(timeout=5);launch('destination');assert notes('destination')==plain
 print(json.dumps({'result':'PASS','checks':['guarded native HTML paste and marks','PNG paste','file URI attachment paste','portable rich copy between two app processes and libraries','attachment ID remap','byte-exact imported assets','plain-text paste','SIGKILL/reopen'], 'scratch':str(root)}))
finally:
 for p in children+owners:
  if p.poll() is None:
   p.terminate()
   try:p.wait(timeout=5)
   except subprocess.TimeoutExpired:p.kill();p.wait(timeout=5)

 for instance_socket in instance_sockets:instance_socket.cleanup()
