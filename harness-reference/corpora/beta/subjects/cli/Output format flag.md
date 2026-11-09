---
slug: cli/output-format
category: cli
description: Which flag picks the output format of a non-interactive run, and which formats exist?
valueSchema:
  description: The output format flag.
  type: object
  properties:
    supported:
      description: 'Whether the harness supports it: true, false, or "partial" where only part works.'
      type:
        - boolean
        - string
      enum:
        - true
        - false
        - partial
    flag:
      description: The flag, such as --output-format.
      type: string
    formats:
      description: Values the flag takes, mapped to what they print.
      type: object
      additionalProperties:
        type: string
    default:
      description: The format used when the flag is absent.
      type: string
    stream_flag_extra:
      description: Further arguments a stream format needs, such as ["--verbose"].
      type: array
      items:
        type: string
    note:
      description: A caveat the other keys do not carry.
      type: string
  additionalProperties: true
---

A headless run can print plain text, one JSON document or a stream of JSON events. This subject records the flag that selects the format, the values it takes and the one used for a stream. The shape of the structured result itself belongs to the structured-output subject.
