# Agent Instructions

## Development Policy

This project has one user. Always make full cutovers: prefer code quality and
simplicity over backwards compatibility, compatibility layers, or parallel old
and new paths.

## MWI Automation Boundary

CDP access to Milky Way Idle is read-only.

Agents may inspect the client, read browser-local state, observe network data,
and export data for simulator development.

Never automate gameplay actions or anything that mutates game state. Do not
click, type, submit, place/cancel/modify orders, start actions, claim rewards,
change equipment, consume items, or send state-mutating authenticated requests
or WebSocket messages.

The only allowed direct server request is reading the public official market
data API. When unsure, stop and ask.
