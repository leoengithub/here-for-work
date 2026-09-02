# HFW-BROWSER-25 live browser evidence

Date: 2026-09-03 (Europe/Madrid)

Scope: isolated public-form inspection and reversible synthetic read-back only. No CV was
uploaded, no personal data was entered, and no submit control was clicked. Both named browser
sessions were closed after verification.

## Greenhouse

- URL: <https://job-boards.greenhouse.io/gleanwork/jobs/4006733005>
- Page remained the Software Engineer, Frontend application form.
- One form, 21 visible native/custom fillables, and three iframes were observed.
- The `Submit application` control remained enabled and untouched.
- The visible First Name field accepted the synthetic value `HFW Browser QA`, read back exactly,
  and was cleared immediately afterward.
- Final visible non-empty field count: zero.
- Screenshot: `greenhouse-public-form.png` (captured before the synthetic check).

## Lever

- URL: <https://jobs.lever.co/drivetrain/5ebd7e09-3f2b-4c53-a0be-0649bdd84842/apply>
- Page remained the Frontend Engineer application form.
- One form, seven visible fillables, and three iframes were observed.
- The `Submit application` control remained enabled and untouched.
- The visible Full name field accepted the synthetic value `HFW Browser QA`, read back exactly,
  and was cleared immediately afterward.
- Final visible non-empty field count: zero (the site's own hidden account/timezone fields are
  intentionally excluded).
- Screenshot: `lever-public-form.png` (captured before the synthetic check).

## Boundary of this evidence

The isolated QA browser exposes WebDriver, which the HereForWork extension intentionally rejects.
Therefore these live checks prove current public-form shape, reversible native-field read-back,
and untouched submit controls; they do not claim a native-host/extension end-to-end run on the
public roles. Extension event settling, exact per-field result cardinality, read-back hashing,
lease transfer/release, and no-submit behavior are covered by the local extension and Rust tests.
