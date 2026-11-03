# Interaction patterns

For whoever draws into a view: what an application's surface is made of, what happens in order between a person acting and something appearing, and which decisions belong to the application rather than to the renderer. It is not the accrete syntax manual (that is `external-integration.md`) and it describes no particular application — only the shape.

The premise behind every section: **the renderer holds no business logic.** It executes operations; it does not interpret actions. "The message I sent appeared" and "submitting the form showed a line of notice" therefore cannot be inferred by a client, and cannot be optimistically inserted by one either: a client does not know that a message is a row, which list that row goes in, or what it looks like. **Display logic belongs to the application; mechanism belongs to the renderer.**

## 1. A base view, and a series of operations on it

What this library gives an application is **an operable base view**: one view first — a surface to act on — and everything afterwards is a series of operations against it (`create`, `set`, `append`, `remove`, `patch`). That is what the name Accretes: the view accumulates, it is not rebuilt per change.

How the base view reaches the client has nothing to do with accrete:

- Over a socket, the natural shape is to push it when the connection opens — there is no "request" to speak of; the connection is the start.
- A conventional application can just as well keep the base view in a file and send it when the user asks for the home page.
- Nor is the socket required anywhere else: the base view can carry a link, and following it requests ANOTHER view that replaces the current one. Replacement is an operation, not a new mechanism.

Equally outside the view is **who produces the operations** — one process, several controllers, several services; where they are spread changes nothing about the view. The view answers to the shape of an operation, not to the shape of the organisation producing it.

Three invariants hold regardless of all of the above:

- **A slot name is a sentence the base view's author wrote** ("my surface reads here"), so only that author can say whether it is true — which is why names are declared by whoever serves the base view and never guessed by whatever draws into it.
- **The base view is not modified by the operations** — operations address slots and rows that it declares, and a name it never wrote is an error, not a new slot.
- **Row identity lives in the data** (`data.id`), so operations are idempotent against the row they name rather than against a position that may have moved.

## 2. One exchange, five steps

The order below is the order of a message in a conversation, which is one case of the shape:

1. **The client sends an event.** The payload carries what the person did (the text), plus — if this side wants a drawing — the slots to draw into (`rack` = the list of rows, `text` = a region). The client draws nothing.
2. **The application turns the act into a fact first.** It writes it down, and that write is where the id of this exchange is minted. Without this step, everything spoken afterwards has no ground.
3. **The application speaks before it works.** First a receipt (`{turn}` — the thing that lets a client tie the sentence it sent to the answer that follows), then ONE batch of operations: append the person's row to `rack`, set up the region the answer will stream into, and append the bubble that reads that region. Order inside the batch is meaning: a patch filling a region must come after the `set` that creates it.

The id has to travel INSIDE the payload, not only beside it: what a host hands a client is the payload (`{ev, args}`), so an id written next to `data` never arrives. A drawing is the exception by nature — an array of operations addressed by slot and row has nowhere to put an id — which is why two exchanges in flight must not aim at one region.
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
- **Per-exchange slot names, today.** The slots are named by the caller when it submits, i.e. before the id exists, so two exchanges in flight share one region and their patches interleave. Closing that means the application deriving the names from the id it minted and stating them in the receipt — the same rule as everywhere (names are declared by whoever serves the layout), not yet the code.

So "does the person see the message they sent" is answered by: yes, because the application sent that row the moment they sent it — not because a client guessed.

## 5. What this asks of an application

1. Write the fact first, speak the drawing after — otherwise the drawing claims something without ground.
2. Speak the drawing BEFORE working — otherwise a slow call looks like "nothing happened after I sent it", and a failed one looks like "I sent it and it vanished".
3. Key every frame of one exchange with the same id (a turn's `turn`), so a client can hang the rest of that exchange on the row it already has.
4. Speak only what a person needs to read (gravity ADR-0010: a tool call's arguments and result stay in the log; at most a title and a status reach the screen).