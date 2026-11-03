# Interaction patterns

For whoever draws into a view: what an application's surface is made of, what happens in order between a person acting and something appearing, and which decisions belong to the application rather than to the renderer. It is not the accrete syntax manual (that is `external-integration.md`) and it describes no particular application — only the shape.

The premise behind every section: **the renderer holds no business logic.** It executes operations; it does not interpret actions. "The message I sent appeared" and "submitting the form showed a line of notice" therefore cannot be inferred by a client, and cannot be optimistically inserted by one either: a client does not know that a message is a row, which list that row goes in, or what it looks like. **Display logic belongs to the application; mechanism belongs to the renderer.**

## 1. An application is its surface

An application's WHOLE surface — what it shows and how it is configured — is the same two things: layouts and events. Nothing about management is a different kind of thing.

- **The initial layout is pushed, not requested.** A client cannot know when it is ready; the venue states what it has on connect and the client draws whatever arrives.
- **It is an EVENT that produces that layout, not a file and not a call.** The trigger is the connection notice (`ui:connect`); whichever booth declared it answers with frames. There is NO deployment-side layout configuration: the file form is gone (a layout with no logic is still an answer its carrier can produce, so a file only means the application is not answering), and the event name is fixed, so there is nothing left to name in an environment.
- **Why an event and not `invoke`.** `invoke` is CALL semantics: one named target, one answer the caller waits for — right for special cases, wrong for composition. Several booths own different parts of one application, so the trigger has to be something every interested booth can receive, each answering with its own frames.
- **Later layouts are ordinary traffic.** An application emits frames at any moment: `create`, `set`, `append`, `patch` against slots it declared earlier.
- **Expansion is a control and an event.** A layout names a control whose `dest` is an application event (a button "manage models", say); the event reaches the booth; the booth emits the frames that create or append that section; its controls emit further events. The venue cannot tell a management section from a chat rack and is never asked to.
- **So there is no admin surface per application**: no admin HTTP verb, no config file, no second origin. `/admin` on the venue is only about the VENUE (node approval, device revocation) — things about connections, devices and the plane itself.
- **A program is a client too.** Automation opens its own connection, gets its own device, and submits the same events with the same payloads; "operable by an agent" is a property of this shape, not an extra API.
- **The layout is the only thing a frame can be checked against.** A slot name is the sentence "my surface reads here", and only the party holding the layout can say whether that sentence is true — which is why the names inside a rendering are declared by whoever serves the layout, never guessed by the executor that draws into it.

## 2. One exchange, five steps

The order below is the order of a message in a conversation, which is one case of the shape:

1. **The client sends an event.** The payload carries what the person did (the text), plus — if this side wants a drawing — the slots to draw into (`rack` = the list of rows, `text` = a region). The client draws nothing.
2. **The application turns the act into a fact first.** It writes it down, and that write is where the id of this exchange is minted. Without this step, everything spoken afterwards has no ground.
3. **The application speaks before it works.** First a receipt (`{turn}` — the thing that lets a client tie the sentence it sent to the answer that follows), then ONE batch of operations: append the person's row to `rack`, set up the region the answer will stream into, and append the bubble that reads that region. Order inside the batch is meaning: a patch filling a region must come after the `set` that creates it.
4. **Only now does the work start.** Each piece of the answer only patches the region step 3 created; nothing re-draws the stage.
5. **The end is said.** `done`, or `error` with its reason. A failure does not undo step 3: the person's sentence staying on screen is the truth.

A button or a form is the same case: a click sends an event, the application answers with operations (perhaps only appending a line of notice). No step of it is something a client could have completed on its own.

## 3. Container shape belongs to the application

Rows share one `rack`; what distinguishes them is each row's own container — in a conversation the person's row is end-aligned and accented while the answer's stretches and carries no accent. That difference **is a class list the frame carries**, not knowledge the renderer has: a renderer that has never heard of a conversation still draws both correctly.

Row identity lives in the data (`data.id`), a reusable layout lives in `rack.item` (ADR 0020), and arrangement lives in `attrs.view` (ADR 0024). The application decides what a row is and what it holds; the renderer decides how those rows are arranged and how they enter and leave.

## 4. What a frame deliberately does not carry

- **How long a row stays, whether it fades, whether the list pins to the newest row.** Those are the renderer's vocabulary (ADR 0024); the application only sends rows.
- **Type names like "notice", "banner", "chat bubble".** They are combinations of values, not types (ADR 0024 §8).
- **A local optimistic insert.** See the premise: a client does not know what the thing should look like.

So "does the person see the message they sent" is answered by: yes, because the application sent that row the moment they sent it — not because a client guessed.

## 5. What this asks of an application

1. Write the fact first, speak the drawing after — otherwise the drawing claims something without ground.
2. Speak the drawing BEFORE working — otherwise a slow call looks like "nothing happened after I sent it", and a failed one looks like "I sent it and it vanished".
3. Key every frame of one exchange with the same id (a turn's `turn`), so a client can hang the rest of that exchange on the row it already has.
4. Speak only what a person needs to read (gravity ADR-0010: a tool call's arguments and result stay in the log; at most a title and a status reach the screen).