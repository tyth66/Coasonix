# /coagent:review-gate

Enable or disable the stop-time review gate. When enabled, the model should run a review after each significant code edit before stopping.

## Arguments

- `enable`: turn on the review gate
- `disable`: turn off the review gate
- (no argument): show current status

## Workflow

1. If `enable` or `disable` is provided, update the plugin config via the companion script.
2. If no argument is provided, show whether the review gate is currently enabled.
3. When enabled, the Coagent skill (injected into model context) reminds the model to run `/coagent:review` after code edits.
