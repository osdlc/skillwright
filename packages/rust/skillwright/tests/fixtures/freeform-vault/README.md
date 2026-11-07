# freeform-vault

**The awkward one.** Everything legal that is not the shape the docs show, so
the support answers can be tested here.

```text
skills/house-style.md              promoted above every marketplace
references/                        a peer directory: no kind folder, untouched
  tone.md                          reached by wikilink, copied at publish
  ideas.md                         no name, no description — skipped in silence
marketplaces/
├─ acme/                           has a note, carrying owner and author
│  ├─ acme.md
│  ├─ skills/shared-check.md       promoted to the marketplace
│  └─ plugins/
│     ├─ core/
│     │  ├─ core.md
│     │  ├─ skills/
│     │  │  ├─ review.md           narrowed to one target · foreign `license` key
│     │  │  ├─ secret.md           `targets: []` — publishes nowhere
│     │  │  └─ bundled/            owns a directory · embeds, links, dangles
│     │  │     ├─ bundled.md
│     │  │     └─ scripts/run.sh   the execute bit travels
│     │  ├─ commands/
│     │  │  └─ ship.md             a command — discovered, never published
│     │  └─ agents/scout.md        an agent — same
│     ├─ noteless/skills/quick.md  a plugin with no note of its own
│     └─ side/skills/stray.md      another one
└─ labs/plugins/spike/skills/probe.md   a marketplace with no note either
```

## What each one is here to prove

**Promotion, at both levels.** `house-style` is shared by every marketplace;
`shared-check` by acme's plugins. Same `skills/` folder, different height, and
the height is the scope.

**A container need not carry a note.** `noteless/` is a plugin and `labs/` is a
marketplace because of what they hold, not because anything says so. The tree
draws both.

**`targets`, all three readings.** Absent on most (`all`), an empty list on
`secret` (`none`), one name on `review`. The badge cannot separate them alone,
which is why the row carries the reading as a hover.

**Kinds that never publish.** `ship` and `scout` are in the vocabulary with no
renderer, so both are listed and both are refused by name rather than written
wrongly.

**Everything a note can reach.** `bundled` embeds `review`, links `ship` and
the peer note `tone`, and names `house-rules`, which nothing answers to. Those
are the four things the Related tree groups — an embed, a link, a peer file and
a dangling name — plus the plugin and marketplace holding it.

**Frontmatter the model does not own.** `review` carries `license: MIT`, which
survives a round trip untouched and is re-emitted at publish. skillwright's own
keys are what gets stripped, not everything unrecognised.

## Nothing here is malformed

Break one by hand — set `name: Not Legal` — and that note alone drops out of the
tree. The others still list. Worth seeing once, because a note that is simply
absent is easy to miss.

To see a scan refuse outright, give two notes in one plugin the same published
name. Two components cannot claim one publish directory, so the whole tree is
replaced by the refusal's diagnosis. That and a missing root are the only
things that stop a scan.
