---
name: to-questionnaire
description: >
  Turn a decision the user cannot answer into a questionnaire document for a
  third party to fill in (async or on a call). Use when the knowledge that
  would settle a question lives outside the repo and outside the user — a
  colleague, an expert, a vendor. Composes with domain-modeling when a
  grilling question hits a fact the user does not hold. Skip for questions
  the user can answer, and skip pure synthesis (to-spec).
version: 1.0.0
source: mini-agi repo (.agents/skills)
disable-model-invocation: true
verify: sh -c "test -f .agents/skills/to-questionnaire/SKILL.md && grep -q 'grill the send' .agents/skills/to-questionnaire/SKILL.md && grep -q 'questionnaire-template' .agents/skills/to-questionnaire/SKILL.md"
---

Turn something the user cannot answer alone into a **questionnaire** — a
Markdown document handed to one person to fill in async, or filled out
together over a meeting. The recipient holds knowledge the user lacks; the
questionnaire pulls it out of them.

**Grill the send, not the subject.** Interview the user only about the _send_,
which they can always answer: who it goes to, and what they need back. The
questions in the document then target the **gap** between what the recipient
knows and what the user needs.

## Process

1. **Who is it going to?** Ask, in one exchange, the recipient's role,
   expertise, and relationship to the user. This fixes the questionnaire's
   tone and how much context it must carry.

   **Done when:** you know who the recipient is and what they know that the
   user does not.

2. **What do you need back?** Ask, in one exchange, the specific decisions
   or facts the user cannot resolve alone and needs from this person.

   **Done when:** you have a concrete list of what the user must walk away
   able to do or decide.

3. **Write the questionnaire.** Draft questions aimed at the gap from steps
   1-2, following the document structure below. Write it to
   `to-questionnaire-<slug>.md` in the current directory (slug from the
   topic) and report the path.

   **Done when:** the file exists and every item the user named in step 2 is
   covered by a question.

## Document structure

Frame the document as a **discovery questionnaire**: the user lacks context,
the recipient holds it. Order questions most-important-first — async means
you may only get one pass — and group them under `##` headings by theme once
there are more than a handful. Every question is **one idea** — never
compound — with an answer stub directly beneath, and a one-line _why this
matters_ only where the question could be misread or invite a throwaway
answer.

<questionnaire-template>

# <Questionnaire title>

**Purpose:** why this questionnaire exists and the decision riding on it.

**From:** <the user> — **To:** <the recipient> — **How your answers will be
used:** <where they go>

## Context

One paragraph orienting a recipient who was not in the user's head. Enough
to answer well, not a page.

## How to answer

Deadline and rough effort. Partial answers and "I don't know" are useful —
flag anything you are unsure of rather than skipping it.

## <Theme heading>

One `##` section per theme. Under each, its questions, most-important-first.

<question-example>
### What load is the system expected to handle at launch?

_Why this matters: it decides whether we provision for burst traffic now or
defer it._

>
</question-example>

## Anything else?

A closing catch-all: anything we did not ask that we should know?

</questionnaire-template>

## Completion criteria

- [ ] The questionnaire exists at `to-questionnaire-<slug>.md` in the
      current directory and every item the user named in step 2 is
      covered by a question (checkable: the file path + the named
      items).
- [ ] The document follows the template structure — purpose, from/to,
      context, how-to-answer, theme headings, catch-all — with one
      idea per question (checkable against the output file).
