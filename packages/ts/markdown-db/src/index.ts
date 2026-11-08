/**
 * markdown-db, version 0 (docs/standards/S-GFFY-markdown-db.md): find corpora and validate them.
 */
export { type CorpusReport, type Data, findCorpora, idProblem, validateCorpus } from './validate.ts'
export { encodePath, fold, markdownLink } from './links.ts'
