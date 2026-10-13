# 0014 — Launch through the existing Rust library owner

Date: 13 October 2026  
Status: implemented in the preview; broader desktop activation acceptance pending

## User experience

Opening Notes again should return to the notebook without disturbing the current
thought. `--new` opens a committed new note; `--note UUID` selects a known note;
`--quick-note` opens or reuses the small writing window. Unknown IDs leave the
current draft and selection alone. Quick-to-main handoff retains the editor,
caret and undo history and refuses during composition or pending insertion.
Default reopening preserves selection and recreates main if Quick is still open.

Beauty and delight include continuity: no duplicate windows, lost caret, surprise
blank notes, blocking desktop roundtrips or false success messages. This increment
uses the existing quiet writing surfaces rather than adding another launcher UI.

## Ownership and replies

The CLI checks the canonical library's private Rust instance endpoint before
initializing GPUI. It forwards even with no graphical/session environment. If the
endpoint is absent, it attempts the compatible exclusive library lock. A startup
race retries only before any request bytes have been sent, for at most 3 seconds.
A reply/read/write failure after sending is an unknown outcome, never permission
to launch a second writer or repeat `--new` automatically. Qt remains separate;
the Rust client does not interpret Qt's receipt-only `ok` as success.

Protocol version 1 gains a tagged Launch command with a validated intent and
optional opaque activation token. Existing capture/status wire forms are unchanged;
older owners reject unknown launch commands. The library owner queues at most
eight UI requests, dispatches one at a time and expires requests not started
within the server's 30-second reply budget. A full/disconnected queue reports
unconfirmed. The existing same-user credentials, bounded frames and private
canonical-path socket rules apply. Capture-only owners reject window requests.

**Opened** is distinct from capture's **Committed**. It identifies the window
intent and optional note, plus whether activation was requested. It is not a
focus grant or a durable capture receipt. A New response waits for the matching
writer operation's committed Created event; a failed create reports its error.
Quick may identify an untouched in-memory draft. Replies are checked against the
request ID, action and requested note before the CLI accepts them.

A pure core guard retains up to 128 owner-local request outcomes, with pending
work protected from eviction. An identical retained ID/payload/token returns its
original outcome, including failure. Reusing an ID with different input fails.
This protects a recent lost reply to the same owner, not a retry after restart or
eviction. The public CLI generates fresh launch IDs and advises checking the app
after an ambiguous result. Durable capture keeps its stronger receipt/status
contract. Replaying Opened describes the original action, not the window's current
existence. Two deliberately separate `--new` invocations create two notes.

## Activation and native handle lifetime

Initial startup leaves token consumption to pinned GPUI. Forwarded tokens are
bounded (4 KiB), reject control characters, and are redacted in Debug output.
The owner never mutates its process environment and never calls a compositor
focus override. Without a token it uses GPUI's normal activation request.
Forwarded window creation disables implicit focus requests; activation follows
the completed action. Wayland's policy for mapping a new toplevel still belongs
to the compositor.

GPUI at `a84689073d296dfd39987bc7dd478e43ef76d83a` exposes raw Wayland window/display
handles but no forwarded-token activation method. `notes-platform::activation`
uses the same libwayland client backend to borrow that display with an independent
event queue. It requests the registry and sync callback, dispatches only already
pending events from GPUI's socket reader, binds xdg-activation v1, and submits the
provided token against the target wl_surface. Flush handles WouldBlock and retry;
there is no blocking read or roundtrip on the UI thread. A two-second deadline
bounds discovery/flush waiting. No GPUI fork or vendored patch is required.

The unsafe boundary is explicit: the host display must outlive this foreign
backend. UI state owns exactly one pending activation. Main close, Quick close
and Quick handoff cancel and drop it before removing the native target. A
synchronous on_app_quit hook drops it before pinned GPUI App::shutdown clears
windows. The app never switches display backends. Surface pointers are wrapped
only inside a live window update, never retained or destroyed by the adapter.
The foreign backend destroys its own queue/proxies and never disconnects the
borrowed display. Revisit these assumptions when updating GPUI.

TokenRequested means the protocol request flushed, not that focus was granted.
Missing protocol, unsupported backend, timeout or failure leaves the opened
window available with ActivationUnavailable. A closed target instead reports
unconfirmed. Invalid/expired token policy belongs to the compositor; the app does
not retry with a newly minted token or force focus. X11 uses the standard GPUI
activation path when no Wayland token is supplied.

## Verification and limits

Core/protocol tests cover bounded replay, mismatched payloads/tokens, redaction,
malformed IDs, queue exhaustion, capture-only owners, UI reply timing and rejection
of mismatched Opened/Committed responses. The real-font GPU workflow covers
committed creation, failure, expiration, dropped-reply replay, draft/caret retention,
Quick reuse, guarded composition/handoff, main recreation and persisted content.

The native oracle uses the release binary and a disposable library. Input/focus is
guarded by owned PID, application ID, exact address and current workspace; text is
acknowledged through AT-SPI. It checks display-free CLI forwarding and real editing,
canonical aliases, two concurrent Quick calls, lost protocol replies, main close/
reopen and Quick handoff. Test-only invalid tokens are traced to distinct main/
Quick surfaces, including the CLI environment path, and do not force main focus.
Those traces prove target submission and invalid-token refusal, not valid-token
focus grants. The installed AT-SPI cache ABI warnings remain a known test limit.

A separate native check sends two simultaneous first-launch `--new` invocations:
two notes commit, only one GUI owner remains, and later display-free requests
reuse it. It also checks explicit-note startup, an unknown startup target and
truthful failed creation with a responsive owner. The existing capture regression
additionally checks that a capture-only owner cannot claim to open a window.

Valid-token focus policy across launchers/desktops, X11, installed desktop actions,
real IME, Orca, scaling and suspend/resume remain acceptance work. This increment
does not change the public Qt executable, installer or production app identity.
