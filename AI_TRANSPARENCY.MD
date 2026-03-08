# AI Transparency

This project was created with significant use of generative AI.

`navtui` was developed through a human + AI workflow where humans set product direction, constraints, acceptance criteria, and release decisions, and a generative AI coding agent produced substantial portions of implementation and documentation.

## What AI Was Used For

- Initial code scaffolding and module structure
- Feature implementation iterations
- Refactors and cross-platform adjustments
- Test generation and documentation drafting

## Human Responsibility and Review

- Humans defined requirements and behavior contracts.
- Humans validated runtime behavior on real environments.
- Humans reviewed, accepted, or rejected generated changes.
- Humans are responsible for release decisions and project stewardship.

## Limits and Risks

Generative AI can introduce:

- Logic bugs
- Security mistakes
- Incomplete edge-case handling
- Incorrect assumptions in docs

For this reason, all releases should continue to pass:

- automated checks/tests
- manual smoke tests
- explicit security review for auth, network, and secret handling paths

## Reporting Issues

If you identify a defect or unsafe behavior, open an issue with:

- environment details
- reproduction steps
- expected vs actual behavior

Transparency and correction take priority over preserving generated output.
