/**
 * markdown-db, version 0 (docs/standards/S-GFFY-markdown-db.md): find corpora, read their data and
 * validate them, and the link helpers the validator uses.
 */
export {
  type CorpusReport,
  findCorpora,
  idProblem,
  readData,
  ROOT_FILES,
  validateCorpus,
} from './validate.ts'
export {
  encodePath,
  extractLinks,
  fold,
  FORMATS,
  type Link,
  markdownLink,
  matchWikilink,
  RECORD_EXTENSIONS,
  wikilinkIndex,
  type WikilinkIndex,
} from './links.ts'
