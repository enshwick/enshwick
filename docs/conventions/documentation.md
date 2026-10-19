# Documentation

> Documents live in the repository, in English, short and plain, and describe
> what exists.

## Where things go

| Path                          | Contents                                                                       |
| ----------------------------- | ------------------------------------------------------------------------------ |
| `README.md`                   | for players first: what Enshwick is, the mods, how to install                  |
| `docs/mods/<id>.md`           | one page per mod: what it changes, its settings, how it behaves                |
| `docs/modding/`               | for mod authors: the API, the manifest, building a mod                         |
| `docs/research/`              | studies with sources, and the owner's decisions taken on them                  |
| `docs/decisions/`             | decision records ([decisions.md](decisions.md))                                |
| `docs/specs/`, `docs/plans/`  | `YYYY-MM-DD-<topic>.md`: a design before it is built, and the plan to build it |
| `docs/conventions/`           | the rules for code, repository, documents and decisions                        |
| `docs/ideas.md`               | mod ideas waiting for their turn                                               |
| `docs/roadmap.md`             | what is done, in progress and next, updated with every finished piece of work  |
| `CHANGELOG.md` beside a crate | written by release-please from conventional commits; never edited by hand      |

A finding worth keeping goes into the document that will act on it, at once.
Nothing of value lives only in a chat or a scratch file.

## How a document reads

- Short, plain and direct, in prose wherever a sentence does the job. Tables for
  facts with the same fields, lists for parallel items.
- It describes what exists. Alternatives that lost belong to the decision record
  that weighed them, and nowhere else.
- No status fields, readiness notes or "coming soon". A spec or plan is dated by
  its file name. The roadmap and the checkboxes of a plan being built are the
  exceptions, since every commit of that work updates them.
- The first mention of a term, another document or a crate links to it.

## Sources

Every claim about the game names where it comes from: the game's own reflection
data, a test on a server, or a reverse-engineering note kept under `research/`
with the game build it was made on. Claims about Rust, Windows or a library cite
their documentation for the version in the stack table of
[repository.md](repository.md).

## Changing documents

- A convention changes only at the owner's request or with the owner's
  agreement, in a commit of its own, never in passing with other work.
- When a subject is gone, its document goes with it, and the documents that
  pointed at it are corrected in the same commit. A superseded decision record
  stays, with a link forward.
- Documents pass `bun run lint:md` and `typos` like any other change.
