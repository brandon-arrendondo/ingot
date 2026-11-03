# ADR-0001: A published artifact cites its sources, ships as a sanitized export, names its accountable reviewer, and discloses AI use

**Status:** Accepted, 2026-11-03 (Brandon Arrendondo).

## Context

Ingot publishes source code, documentation, release archives, and platform
packages. Correct output does not by itself make an artifact ready to publish.
Citations, licences, private working material, and responsibility need the
same review.

## Decision

Before an artifact is submitted or published, it must meet four conditions.
An artifact is anything that leaves BISSELL: the public repository and its
site, a release archive, dataset, paper, or book.

1. **It cites sources correctly and is free of plagiarism.**
   - Verify every citation against the primary source: the work itself first,
     then its DOI registry record. Do not write a citation from memory or a
     model's output. Verify authors, title, date, and publication details.
   - Learn from sources without copying material their licences do not allow.
     Identify quotations as quotations and cite them. This applies to text,
     figures, and code.
   - A derived file retains its original licence notice and records what
     changed, who changed it and when, and where the unmodified original is.
2. **It is released only as a sanitized export.**
   - Submit a clean archive containing only what the recipient needs to build
     the artifact, in the recipient's required form, together with the final
     output such as the release build. Do not submit a working repository or
     incidental build artifacts.
   - Export no personal information or intellectual property beyond what is
     intended for publication. Internal machine names and hostnames, IP
     addresses, local filesystem paths, tracking references, and internal
     URLs do not ship. Do not locate third-party defects that have not been
     fixed upstream.
   - The exception is authorship already made public by Git: commit author
     names and email addresses, including Brandon Arrendondo
     (brandon.arrendondo@bissell.com). Name another BISSELL Associate only
     with permission obtained before submission.
3. **It names the accountable BISSELL Associate.** Record who holds final
   review responsibility with the artifact. That is Brandon Arrendondo unless
   the artifact explicitly names someone else.
4. **It discloses AI use in the form appropriate to the artifact.** Use the
   README's AI Assistance section for the tool, front matter for a book, and a
   statement in each paper. Commit trailers are not the disclosure mechanism.
   Contributor attribution rules remain in AGENTS.md.

Every artifact goes through **BISSELL legal review before submission or
publication**. **BISSELL Homecare, Inc. is the primary copyright holder**.
Ingot is licensed under **MIT**, as recorded in `LICENSE`. Its release
workflow generates `THIRD_PARTY_LICENSES.txt` with cargo-about and a CycloneDX
JSON software bill of materials, and includes both in the platform release
packages. The MIT licence for Ingot does not replace an incorporated source's
own licence obligations. Contributions require the DCO sign-off recorded in
AGENTS.md; a sign-off does not substitute for the accountable Associate's
artifact review.

These conditions carry out the BISSELL AI Acceptable Use Policy's principles
of accountability, data protection, intellectual property, data quality, and
AI attribution.

## Consequences

- This is policy, not the export procedure. Maintainers maintain the
  pre-publication checklist and sanitized-export process; procedural changes
  do not relax these conditions. This ADR records no artifact as having
  completed legal review; automation cannot satisfy it.
- The repository is already a published artifact. Citation, sanitization, and
  disclosure conditions apply to every commit reaching origin, not only to
  tagged releases. Existing contributor and changelog obligations continue to
  apply.
- Where source material is not public, sanitize at export. Keep verification
  notes and working records in their source workspace; do not strip them in
  advance. A working note that prints into the artifact, for example through
  a bibliography field, is fixed in the artifact.
- Verify a citation when added and again when its artifact is submitted or
  published; this does not require checking every unchanged citation again for
  every tool release.
- Fix a leak found in published text at the head. Whether to rewrite Git
  history or replace an already-published artifact is a separate maintainer
  decision; this ADR grants no blanket history-rewrite authorization.

Origin: adaptation of knots ADR-0006, itself adapted from aurora-lint
ADR-0016. The shared publication conditions apply equally; Ingot uses MIT.

Source record: [knots ADR](https://github.com/brandon-arrendondo/knots/blob/7dc71958c024b08726de5a095fa4d94abddbce95/docs/adr/0006-published-artifacts.md).
