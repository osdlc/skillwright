# Decisions

One record per decision. A changed decision gets a new record that
supersedes the old one; records are not edited to reverse them. Before 1.0, an
amendment that narrows a decision is edited into the record in place. Ids are
random keys, so parallel branches do not collide.

| Id | Decision |
|---|---|
| [D-COMG](D-COMG-marketplace-model-and-translators.md) | One marketplace model, translated per harness |
| [D-EPSA](D-EPSA-resolve-at-load.md) | Resolve links and calls when loading, not when translating |
| [D-ISOU](D-ISOU-harness-extensions.md) | Harness-only fields are a validated map per harness |
| [D-FDLQ](D-FDLQ-harness-profile-and-translator.md) | Each harness is a profile and a translator |
| [D-TZFV](D-TZFV-shared-vault-model.md) | Read the vault through osdlc's shared crates |
| [D-GPYR](D-GPYR-convention-first.md) | Structure comes from convention |
| [D-KVMB](D-KVMB-bare-keys-by-default.md) | Frontmatter keys are bare |
| [D-HJCZ](D-HJCZ-harness-blocks.md) | Harness-only fields live in a top-level block per harness (supersedes D-ISOU) |
| [D-WNTQ](D-WNTQ-harness-profile-values-from-config.md) | A harness profile value can come from config (amends D-FDLQ) |
| [D-QXRN](D-QXRN-harness-facts-from-knowledge.md) | Harness profile facts come from the compiled knowledge file (amends D-FDLQ) |

Terms are defined in [../terms](../terms/README.md). Standards are in [../standards](../standards/README.md).

Decisions about the shared vault crates themselves are in [osdlc's decisions](https://github.com/osdlc/osdlc/blob/main/docs/decisions/README.md).
