# SMART-CONTRACT [SC-002]: Add validation rules for Zaps ID registration

## Description
Add rules to username registration: must be lowercase, alphanumeric, and between 3-15 characters long.

## Files to Edit/Create
- `contracts/contracts/user_registry/src/lib.rs`

## Acceptance Criteria
- Error if username contains capital letters or special chars.
- Error if length < 3 or > 15.

## Guidance / Hints
Iterate over characters of the String or use simple ASCII check to enforce alphanumeric.
