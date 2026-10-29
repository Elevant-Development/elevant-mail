# Security

Elevant Mail handles mail content from untrusted senders and account credentials, so security
reports are very welcome.

## Reporting a vulnerability

Please do not open a public issue for security problems. Use GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
on this repository instead. Include the version, your platform and, if possible, a sample mail
or steps that reproduce the problem.

## Scope

Especially relevant:

- Mail HTML that executes script, loads remote content without consent or escapes the sandbox
- Credentials or tokens ending up in files, logs or error messages
- TLS validation that can be bypassed
- Attachments that run without an explicit user action
