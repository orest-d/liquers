---
id: RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND
kind: issue
title: Recipe argument and link overrides do not reach a pipeline command that is the query's last action
status: draft
priority: P3
complexity: S
area: [core/plan]
design: composite-command-definition
created: 2026-11-04
github:
---
# Recipe argument and link overrides do not reach a pipeline command that is the query's last action

## Problem

A recipe's `arguments` and `links` override the parameters of the query's last action:
`Plan::override_value`, `override_link` and `override_pipeline` (`liquers-core/src/plan.rs`) look
for the last `Step::Action`. When the last action is a pipeline command, the plan ends in a
`Step::PipelineCommand` instead, whose arguments are already bound into its body, so an override
finds no parameter of that name and the recipe fails as if the argument did not exist.

## Impact

A recipe cannot leave an argument of a pipeline command open for an override. Workaround: write
the argument in the recipe's query, or call the steps directly.

## Expected behaviour

An override of a final pipeline command rebinds that command's argument, as for a registered
command: the call's parameters are kept on the step until overrides are applied, then the body
is planned.

## Discovery

Found while implementing pipeline command expansion (Step 11 of
`design/composite-command-definition/`).
