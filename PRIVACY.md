# Privacy Policy

**Last updated: February 24, 2026**

ScreenMCP ("we", "us", "our") provides tools that let AI assistants see and control Android phones and desktop computers via the Model Context Protocol (MCP). This policy explains what data we collect, why, and how we protect it.

## 1. What We Collect

### Account Information

When you sign in with Google, we receive your email address, display name, and a unique Firebase user ID. We store this to identify your account, manage your API keys, and associate your devices.

### Device Information

When you connect a phone or desktop, we store a client-generated device ID (a random cryptographic identifier), an optional device name, and device model string. We do not collect IMEI, phone number, or hardware serial numbers.

### Usage Data

We log what types of commands are executed and the count (e.g. "screenshot", "click", "type"), the target device ID, and a timestamp. This is used for daily usage limits, billing, and abuse prevention. We do not log command parameters (coordinates, typed text, played audio, etc.) in the cloud.

### Screen Content

Screenshots, UI tree data, and command responses flow between your device and the AI client through our worker relay servers. This data is transmitted in real time and is **not stored** on our servers. It passes through the relay in memory only and is discarded once delivered. We do not inspect, analyze, or retain screen content.

### API Keys

API keys you create are stored as irreversible SHA-256 hashes. We cannot recover your key after creation — only you have the plaintext.

## 2. How We Use Your Data

- **Authentication** — verify your identity and authorize device connections
- **Service operation** — route commands between your AI client and your devices
- **Usage limits & billing** — enforce plan limits and track usage for paid tiers
- **Abuse prevention** — detect and prevent unauthorized use of the service

We do not sell your data. We do not use your data for advertising. We do not train AI models on your data.

## 3. Android Accessibility Service

The ScreenMCP Android app uses Android's AccessibilityService to execute UI actions (taps, scrolls, text input) and read screen content (UI tree, focused element text) on your device. This permission is required for the core functionality of remote device control.

The AccessibilityService is only active while the app is connected to a ScreenMCP worker. All data accessed via this service is sent only to the worker relay you are connected to, and only in response to commands from an authenticated controller (your AI client). No data from the AccessibilityService is sent to third parties, collected for analytics, or stored on our servers.

## 4. Data Storage & Security

- Account and device data is stored in a PostgreSQL database hosted on our servers.
- All connections use TLS encryption (HTTPS/WSS). Data in transit between your device, the worker relay, and your AI client is encrypted.
- API keys are hashed before storage. Firebase authentication tokens are verified server-side and not stored.
- Screen content (screenshots, UI trees, responses) is relayed in memory and never written to disk on our servers.

## 5. Open Source & Self-Hosting

ScreenMCP offers an open-source, self-hosted mode where all data stays on your own infrastructure. In self-hosted mode, no data is sent to our servers. Authentication is handled locally via a TOML configuration file, and there is no account registration, no usage logging, and no external network calls.

## 6. Third-Party Services

- **Firebase Authentication** (Google) — handles sign-in. Subject to [Google's Privacy Policy](https://firebase.google.com/support/privacy).
- **PayPal** — processes payments for paid plans. Subject to [PayPal's Privacy Policy](https://www.paypal.com/webapps/mpp/ua/privacy-full).

We do not share your data with any other third parties.

## 7. Data Retention

- **Account data** — retained while your account is active. You can request deletion by contacting us.
- **Usage logs** — retained for 90 days for billing and abuse prevention, then automatically deleted.
- **Screen content** — not retained. Relayed in real time and discarded.

## 8. Your Rights

You can:

- **Delete your account** — contact us and we will delete your account and all associated data.
- **Export your data** — request a copy of the data we hold about you.
- **Revoke access** — delete your API keys from the dashboard at any time to immediately revoke all third-party access.

## 9. Children

ScreenMCP is not directed at children under 13. We do not knowingly collect data from children under 13.

## 10. Changes

We may update this policy from time to time. Material changes will be communicated via email or a notice on our website.

## 11. Contact

For privacy questions or data requests, email us at support@screenmcp.com.
