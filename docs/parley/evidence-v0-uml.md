# Claims profile: class diagram

The proposed claims profile of markdown-db, after GA4GH VA-Spec, with harness-reference as the
domain and the optional software overlay. Stereotypes:

- `record`: its own file. `folder record`: a folder holding `<name>.md` and its files.
- `inline`: an object inside another record.
- `domain`: supplied by the corpus, not the profile.
- `overlay`: the optional software overlay, an `allOf` layer over a base class.

Links are relative Markdown links checked with `x-ref`. Inline objects that hold links need nested
`x-ref` (roadmap item 3).

```mermaid
classDiagram
  direction LR

  class InformationEntity {
    <<abstract>>
    +id : file name
    +description : string
    +contributions : Contribution[]
  }

  class Statement {
    <<record>>
    statements/
    +proposition : Proposition
    +direction : supports | disputes | neutral
    +strength : derived
    +evidence_lines : EvidenceLine[]
  }
  class Proposition {
    <<inline>>
    +subject : link → domain record
    +predicate : link → domain question
    +object : value, per predicate schema
    +since : version or date
    +until : version or date
  }
  class EvidenceLine {
    <<inline>>
    +items : link[] → StudyResult | Statement
    +direction : supports | disputes | neutral
    +strength : optional override
    +warrant : string
    +method : link → Method
  }
  class StudyResult {
    <<record>>
    study-results/
    +kind : docs | changelog | source | help-text | run | issue | third-party | inferred
    +document : link → Document
    +method : link → Method
    +observed : date
    +locator : string
    +excerpt : exact, prefix, suffix
    +data : per kind
  }
  class Method {
    <<folder record>>
    methods/name/name.md
    +predicates : link[] → domain question
    +procedure : stages
    +fixtures : files
    +controls : positive, negative
    +safety : string[]
    +maturity : draft | functional | reusable | reproduced
  }
  class Document {
    <<record>>
    documents/publisher/
    +url : https
    +path : file, with sha256
    +title : string
    +publisher : string
    +quality : tier
    +archive : Wayback URL
    +url_status : live | dead
  }
  class Contribution {
    <<inline>>
    +agent : id | link → Agent
    +role : created | verified | disputed
    +at : date-time
  }
  class Agent {
    <<record, optional>>
    agents/
    +kind : person | software | model
    +version : string
  }

  class Harness {
    <<domain>>
    harnesses/
    +vendor
    +homepage, docs, repository
    +license, open_source
    +cli, surfaces, status
    +successor, fork_of : link → Harness
    +sources : link[] → Document
  }
  class Subject {
    <<domain>>
    subjects/
    +question : string
    +value : JSON Schema
    +claim_type : behaviour | policy
  }

  class SoftwareStudyResult {
    <<overlay>>
    observations/
    +subject_version : string
    +commit : sha
    +digest : string
    +environment : os, arch, runtime, model
    +command : string
    +exit_code : integer
    +transcript : file
  }
  class SoftwareMethod {
    <<overlay>>
    probes/
    +entrypoint : string
    +fingerprint : hash of files
  }
  class SoftwareDocument {
    <<overlay>>
    sources/
    +repo : string
    +ref : commit sha
    +lines : range
  }

  InformationEntity <|-- Statement
  InformationEntity <|-- StudyResult
  InformationEntity <|-- Method
  InformationEntity <|-- Document

  Statement *-- "1" Proposition
  Statement *-- "0..*" EvidenceLine
  EvidenceLine --> "1..*" StudyResult : items
  EvidenceLine --> "0..*" Statement : items
  EvidenceLine --> "0..1" Method : method
  StudyResult --> "0..1" Document : document
  StudyResult --> "0..1" Method : method
  InformationEntity *-- "0..*" Contribution
  Contribution --> "0..1" Agent : agent

  Proposition --> "1" Harness : subject
  Proposition --> "1" Subject : predicate
  Method --> "0..*" Subject : predicates
  Harness --> "0..*" Document : sources

  StudyResult <|-- SoftwareStudyResult
  Method <|-- SoftwareMethod
  Document <|-- SoftwareDocument
```

Read it as:

- A **Statement** is the claim. Its **Proposition** names the domain record (a harness), the
  domain question (a subject) and the value.
- Each **Evidence Line** argues for or against the statement from one or more **Study Results**
  (or other statements), with a direction and a reason.
- A **Study Result** is one observation of a **Document** or one run of a **Method**. It points at
  nothing in the domain; the statements that cite it do.
- Every record carries **Contributions**: who created, verified or disputed it, and when.
- **Harness** and **Subject** belong to harness-reference. Another corpus swaps in its own, such
  as models and jobs in small-model-evals.
- The **software overlay** adds version, commit, environment and transcript fields, and renames
  the folders (`observations`, `probes`, `sources`). Its schemas say which class they implement
  with `x-class`.
