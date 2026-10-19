# Decision records

> A record is what an argument leaves behind: the decision, the options that
> lost, and the reason each way. The push closes it.

## What earns a record

A decision earns a record when it moves the structure of the loader or its ABI,
moves a quality attribute such as frame time or crash safety, or costs enough
that reversing it hurts. Choosing a technology, an approach, a pattern or a rule
the project binds itself to qualifies on the same terms. Anything small in
scope, time, risk and cost is built without one.

One record holds one decision. A decision that arrives in phases takes a record
per phase.

## Where and how

A record is `docs/decisions/NNNN-<decision>.md`, numbered in sequence; a number
is never reused. The title states the decision, not the problem. It runs to
about one screen, in full sentences, and design material sits behind a link to a
spec or study.

It opens with its status and the date of the decision, then may give the
argument in one line: in the context of X, facing Y, we decided for Z and
against W, to achieve Q, accepting R.

| Section            | Carries                                                                                      |
| ------------------ | -------------------------------------------------------------------------------------------- |
| Context            | the forces as fact, in neutral words: a constraint of the game, a missing capability, a cost |
| Decision           | one sentence in the active voice                                                             |
| Considered options | every option that stood a chance, at one level, each with what was good and bad about it     |
| Consequences       | what gets better, what gets worse, what stays even                                           |
| Confidence         | how sure the decision is, and what change would reopen it                                    |
| Compliance         | how keeping to the decision is checked: a lint, a test, a check in CI                        |

An option nobody would have taken is left out. A record carries no version
numbers, which live in the stack table of [repository.md](repository.md), unless
the version is itself the decision. A record names no person and no delivery
date.

## Status

- **Proposed** while the argument is open.
- **Accepted** once the maintainer has decided.
- **Rejected** when the option was weighed and turned down, with the reason.
- **Superseded** once another record replaces it. Each of the two names the
  other, and neither text is rewritten.

## The push closes it

Before the push a record is a draft and is rewritten in place. After it, only a
superseding record changes anything.
