# How we work — read before you post

Ten rules. Each one cost us something specific; the scar is there so the rule is remembered rather than recited.

**1. The evidence and the claim go in the same message.**
Not "I checked" — the command and its output. A claim without them is unverified, and nobody should argue with it, just ask for the paste. Every failure below dies on this rule.

**2. Read the artefact, not your memory of it.**
I described "today's" server from a checkout seven commits stale; quoted a claim row after it had been reworded; quoted a price three times without opening the function. `git fetch` before reasoning about current code. Re-read the file before quoting it.

**3. A check that cannot fail is not a check.**
Name the input that would have turned it red, and confirm that input exists. `shasum -c --ignore-missing` with nothing downloaded; a mirror comparison against the same source; a scan whose report path was empty. If you can't produce a red result, you've measured nothing.

And report **the tool's own exit code**, not the last thing in the pipeline: `${PIPESTATUS[0]}` through a pipe, never `$?` after a `| grep`. `$?` after a pipe is grep's status — grep found a line, so it's green, whatever the command actually did. A gate that reports the wrong process's exit code is the same failure wearing a different hat, and it's the most common way this rule gets broken.

**4. Compare against what users get, not what's on `main`.**
The docs check that compared against `main` would have enforced yesterday's mistake. A page claiming shipped behaviour tracks the released artefact. "Tagged" is not "released"; "merged" is not "deployed".

**5. Never draw a conclusion about a live system from a reading you didn't take.**
Not a 30-second sample on an idle fleet, not a port list before measuring what's actually bound, not an assumed filename in a merged diff (`ci.yml` that didn't exist). On a live host: read the hash, don't execute the binary to find out what it is.

**6. Credentials never go in argv, in chat, or in a public comment.**
`ps` shows argv to every same-user process. Pipe it, or use a 0600 config. If you find one in a public place, rotating it is the fix — removal is hygiene.

**7. Public output has a gate, and the gate is a person.**
One owner, taken visibly. Draft in the thread. "Facts OK" on the **final** draft — a partial check of one claim is not a gate pass. And **no disclosures about our infrastructure, incidents or internal setup** in a public reply unless Jeremy explicitly approves it.

**8. Editing is not erasure.**
GitHub keeps comment edit history retrievable, and only a repo admin can delete a revision. A disclosure can't be corrected in place — the only reliable remedy is not posting it.

**9. Before saying "this cannot be checked", list what access you have.**
I declared a verification impossible after two failed fetches while holding a working API client. Two failed probes are not a proof of impossibility, and a false "unverifiable" is worse than an admission of ignorance.

**10. A bound must be re-bounded, and a number must be derived.**
Every limit converts one failure into another: a lease into queue wait, an upload cap into a slot held by one slow client, an ignore file into masked advisories. Say what the bound converts the failure into, and bound that. And never hand-maintain a value that could be derived — `LEASE_SECONDS` as a decorative constant, an upload limit stated three ways, two copies of `install.sh`.


**11. A search used as evidence of absence must be run unfiltered, or counted first, before anything trims its output.**
`| head` doesn't lie when it finds nothing — it lies when the match that mattered was at line 21. If you trim the output, count it first (`| wc -l`) and say the count. (Thomas, 2026-11-18: a `| head -20` grep hid the one live reference behind twenty dead ones; Harry's SIGPIPE framing was the wrong mechanism, and the rule carries his correction too — a plausible mechanism stated with more confidence than the evidence.)

---

**Why a document alone won't fix this.** Every rule above was already known to the person who broke it. These are cost failures, not knowledge failures: the wrong path was faster at the moment it was taken. What changes behaviour is a refusal — someone who can say no, at the point of publishing. Rules 1, 3 and 7 work because another person can enforce them. Keep them where enforcement is possible.

---

## This file is only half of it

The rules above are about **how to behave**. They cannot hold **facts**, and about half of a bad day is forgotten facts rather than broken rules: a number that was 203 and got remembered as 204, a decision Jeremy already made, a policy that changed. No amount of care prevents that, because the person is not being careless — they are being confident.

So the shared file needs a second section, and it has different rules:

**Standing decisions** — what Jeremy has decided, each dated and linked to the message where he decided it. "No Docker, deliberately" is not a preference to be re-litigated; it is a decision with a date.

**Infra facts** — each line in one fixed shape, so it can be read at a glance and can't quietly age:

```
<node> runs <version> · verified 2026-11-18 · `sha256sum /usr/local/bin/<daemon>` → `<hash>`, matched to the <version> release hash · expires 2026-12-18
```

`verified <date> · <command> · result`, plus `expires <date>` — **30 days for infrastructure state, no expiry for Jeremy's decisions**, which stand until he changes them. **A fact past its expiry date counts as unverified**: it doesn't get the benefit of the doubt, and quoting it without re-running the command is the same as quoting a rumour.

Two constraints behind that shape, both learned the hard way:

1. **A fact without a verification date is a rumour.** The reader needs to know whether they're looking at last week's check or last quarter's, and what command re-runs it.
2. **Write facts to expire, not to be true.** The dangerous entry is never the obviously wrong one — it's the one that was right three months ago, wearing the same authority as today's. A stale fact is worse than a missing one, because it kills the question before anyone asks it.

**And this half never goes in a public repo.** The rules are safe to publish — they're generic. Facts aren't: which machine runs what version, what address a node has, who holds a key. A public repo containing the facts section publishes the infrastructure the rules exist to protect, and it does it in the one file everyone has agreed to read. Rules everywhere; facts only where the work is private, with the public copy naming the file and not describing its contents — a description is itself a hint.

Both sections change by **PR with a reviewer**, for the same reason a public reply gets a facts check: a wrong fact in a shared file misleads everyone simultaneously, and it does it silently.
