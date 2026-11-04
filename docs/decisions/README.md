# Decisions

One record per decision. A changed decision gets a new record that
supersedes the old one; records are not edited to reverse them. Ids are
random keys, so parallel branches do not collide.

| Id | Decision |
|---|---|
| [D-COMG](D-COMG-marketplace-model-and-translators.md) | One marketplace model, translated per harness |
| [D-EPSA](D-EPSA-resolve-at-load.md) | Resolve links and calls when loading, not when translating |
| [D-ISOU](D-ISOU-harness-extensions.md) | Harness-only fields are a validated map per harness |
| [D-FDLQ](D-FDLQ-harness-profile-and-translator.md) | Each harness is a profile and a translator |
| [D-TZFV](D-TZFV-shared-vault-model.md) | Read the vault through shared osdlc crates |

Terms are defined in [../terms](../terms/README.md).
