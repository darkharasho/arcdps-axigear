# Release Notes

Version v0.1.3 — October 8, 2026

## Update checks no longer hit GitHub's rate limit

The update check could fail with "status code 403". It used GitHub's
API, which allows only 60 requests an hour from one connection, and
every plugin in every game client shares that limit. The check now
asks GitHub's release page directly, which has no such limit.

If GitHub does turn the check away, the message now says it is
rate-limiting the connection instead of showing a bare status code.
