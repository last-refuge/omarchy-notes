#!/usr/bin/env python3
"""Real CLI + single-writer IPC, with no graphical/session environment.
All libraries, runtime sockets and owner processes belong to this test.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import socket
import sqlite3
import struct
import subprocess
import tempfile
import uuid

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary',type=Path)
parser.add_argument('owner',type=Path)
args=parser.parse_args()
binary,driver=args.binary.resolve(),args.owner.resolve()

def run(root,*arguments,ok=True,input=None):
    result=subprocess.run([str(binary),'--data-dir',str(root),*map(str,arguments)],env=env,input=input,text=True,capture_output=True,timeout=12)
    assert (result.returncode==0)==ok,(result.returncode,result.stdout,result.stderr)
    if not ok: assert not result.stdout.startswith('Saved note '),result.stdout
    return result

def capture(root,text,id):
    result=run(root,'--capture',text,'--request-id',id)
    assert id in result.stderr
    return re.search(r'Saved note ([0-9a-f-]{36})',result.stdout).group(1)

def count(root):
    with sqlite3.connect(root/'library.sqlite3') as db:return db.execute('SELECT count(*) FROM notes').fetchone()[0]

def start_owner(root,fail=False):
    process=subprocess.Popen([str(driver),str(root)]+(['--simulate-save-failure'] if fail else []),env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    selector=selectors.DefaultSelector();selector.register(process.stdout,selectors.EVENT_READ)
    try:
        assert selector.select(10),'Owner did not start'
        assert process.stdout.readline().strip()=='READY',process.stderr.read()
    except BaseException:
        process.kill();process.wait();raise
    finally:selector.close()
    return process

def stop_owner(process):
    try:
        out,err=process.communicate('stop\n',timeout=5)
        assert process.returncode==0,(out,err)
    finally:
        if process.poll() is None:process.kill();process.wait()

with tempfile.TemporaryDirectory(prefix='notes-capture-') as tmp:
    base=Path(tmp);runtime=base/'runtime';runtime.mkdir(mode=0o700)
    env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime))
    for key in ['DISPLAY','WAYLAND_DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS']:env.pop(key,None)
    for option in ['--help','--version']:
        subprocess.run([str(binary),option],env=env,check=True,capture_output=True)
    root=base/'library';id=str(uuid.uuid4());text='A thought from the terminal\nKeep #capture and 👩‍💻'
    note=capture(root,text,id)
    assert capture(root,text,id)==note and count(root)==1
    assert note in run(root,'--capture-status',id).stdout
    run(root,'--capture','different','--request-id',id,ok=False)
    run(root,'--capture',' \n\t',ok=False)
    run(root,'--request-id',id,ok=False)
    run(root,'--capture-status',str(uuid.uuid4()),ok=False)
    missing=base/'missing';run(missing,'--capture-status',str(uuid.uuid4()),ok=False);assert not missing.exists()
    # Invalid window combinations and note IDs fail before opening storage/GPU.
    for options in [('--new','--quick-note'), ('--new','--capture','text'), ('--note','invalid'), ('--note',str(uuid.uuid4()))]:
        run(missing,*options,ok=False);assert not missing.exists()
    stdin_id=str(uuid.uuid4());run(root,'--capture','-','--request-id',stdin_id,input='Piped\r\ntext');assert count(root)==2
    # Raced independent launches use the same receipt without a daemon or window.
    shared=str(uuid.uuid4())
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        ids=list(pool.map(lambda _:capture(root,'Raced direct capture',shared),range(2)))
    assert ids[0]==ids[1] and count(root)==3
    owner=start_owner(root)
    endpoint=runtime/'omarchy-notes-rust'/(hashlib.sha256(os.fsencode(root.resolve())).hexdigest()[:32]+'.sock')
    try:
        before=count(root)
        run(root,'--new',ok=False)  # Capture-only owners never claim a window.
        assert count(root)==before
        alias=base/'alias';alias.symlink_to(root,target_is_directory=True)
        forwarded=str(uuid.uuid4());saved=capture(alias,'Forwarded with no display',forwarded)
        assert capture(root,'Forwarded with no display',forwarded)==saved
        requests=[str(uuid.uuid4()) for _ in range(8)]
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            saved_ids=list(pool.map(lambda id:capture(root,'Parallel '+id,id),requests))
        assert len(set(saved_ids))==8
        # Drop a real socket before its reply. Query the committed receipt, then
        # retry through the public CLI and verify there is exactly one note.
        lost=str(uuid.uuid4());message={'version':1,'request_id':lost,'command':{'action':'capture','text':'Lost response, retained thought'}}
        data=json.dumps(message).encode()
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
            client.connect(str(endpoint));client.sendall(struct.pack('!I',len(data))+data)
        retried=capture(root,message['command']['text'],lost)
        assert retried in run(root,'--capture-status',lost).stdout
        assert count(root)==13
        before=count(root);run(root,'--capture','must not change owner mode','--simulate-save-failure',ok=False);assert count(root)==before
    finally:stop_owner(owner)
    assert not endpoint.exists()
    assert capture(root,'Forwarded with no display',forwarded)==saved and count(root)==13
    backup=base/'backup';restored=base/'restored';run(root,'--backup',backup)
    run(restored,'--restore',backup)
    assert capture(restored,'Forwarded with no display',forwarded)==saved and count(restored)==13
    failed=base/'failure';owner=start_owner(failed,fail=True)
    try:
        request=str(uuid.uuid4());run(failed,'--capture','must not save','--request-id',request,ok=False)
        run(failed,'--capture-status',request,ok=False);assert count(failed)==0
    finally:stop_owner(owner)
    # Simulate a killed owner and reclaim only its dead lock/socket.
    owner=start_owner(root);owner.kill();owner.wait(timeout=5)
    assert endpoint.exists()
    owner=start_owner(root)
    try:assert capture(root,'Forwarded with no display',forwarded)==saved
    finally:stop_owner(owner)
    assert not endpoint.exists()
    print('PASS: display-free direct/forwarded/stdin capture, canonical library aliases, concurrent launch/requests, durable status, identical retries, mismatched IDs, dropped replies, restarted owners, failure truthfulness, stale socket recovery and receipt-preserving backup/restore.')
