# Release Notes

Version v0.2.1 — October 8, 2026

## Fixes

- Fixed a crash in axigear that could happen when one of the game's
  threads shut down. arcdps caught it and the game kept running, but
  the axigear window could stop responding afterwards (for example,
  the comp dropdown did nothing).
