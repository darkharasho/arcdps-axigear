# Release Notes

Version v0.1.2 — October 8, 2026

## Large comps load again

- Comps published by older AxiForge versions can be much bigger than
  current ones, and some showed "offline (response too large)". axigear
  now accepts comp files up to 32 MB.
- A big download is no longer cut off after 8 seconds. It now fails
  only if the connection stalls for 8 seconds, or after 60 seconds in
  total.
- A comp file that is still too large shows "Comp file is too large -
  republish it from AxiForge." instead of retrying over and over.
  Republishing from current AxiForge makes the file much smaller.
