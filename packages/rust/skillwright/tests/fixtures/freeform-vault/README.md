# freeform-vault

**The awkward one.** Everything legal that is not the shape the docs show, so
the support answers can be tested here.

```text
skillwright.md                     sets nothing: every default holds
marketplaces/
├─ acme/                           carries owner and author
│  ├─ acme.md
│  └─ plugins/core/
│     ├─ core.md
│     ├─ references/tone.md        shared by the plugin, reached by wikilink
│     ├─ skills/
│     │  ├─ review/review.md       foreign `license` key
│     │  └─ bundled/               owns a directory · embeds, links, dangles
│     │     ├─ bundled.md
│     │     └─ scripts/run.sh      the execute bit travels
│     ├─ commands/ship/ship.md     a command
│     └─ agents/scout/scout.md     an agent
└─ labs/                           a second marketplace
   ├─ labs.md
   └─ plugins/spike/
      ├─ spike.md
      └─ skills/probe/probe.md
```

## What each one is here to prove

**Two marketplaces.** A build writes `dist/acme/` and `dist/labs/`, and
`--marketplace acme` builds one.

**Kinds a pack may not carry.** `scout` is an agent, which no harness's pack
holds, so it is named rather than written wrongly.

**Everything a note can reach.** `bundled` embeds `review`, links `ship` and
the shared reference `tone`, and names `house-rules`, which nothing answers
to.

**Frontmatter the model does not own.** `review` carries `license: MIT`, which
survives a round trip untouched and is re-emitted by the build. skillwright's own
keys are what gets stripped, not everything unrecognised.
