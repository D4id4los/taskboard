# Embedded print fonts (PDF export, issue #624 stage B)

`NotoSans-Regular.ttf` and `NotoSans-Bold.ttf` (Noto Sans, Google /
the Noto project) are embedded into the `taskboard` binary via
`include_bytes!` — only in builds with the `pdf-export` feature
(default off). Nothing ever loads a machine-local font path.

- **Provenance**: Debian `fonts-noto-core` package (hinted), files
  dated 2020-12-26, upstream <https://github.com/googlei18n/noto-fonts>.
  - `NotoSans-Regular.ttf` sha256
    `89c3c497f618fdaa0b2d1e98fef93582f28c71debd2c4a8cdf41f190ced2909d`
  - `NotoSans-Bold.ttf` sha256
    `e83493c945848ecd4a9ad0f6d19164541a0d3e23a9c952304a00a46e00272ac5`
- **License**: SIL Open Font License 1.1 (`OFL.txt`, verbatim).
  Embedding and redistribution inside an AGPL-3.0-only binary is
  permitted; the OFL only forbids selling the font by itself and
  requires keeping the copyright notice and license text with every
  copy — this directory is that record.
