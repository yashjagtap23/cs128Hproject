# Running the Coffee Chat Automator

This guide provides instructions on how to set up, configure, and run the Coffee Chat Automator application.

## 1. Prerequisites

- **Rust:** Ensure you have Rust installed. If not, install it from [rust-lang.org](https://www.rust-lang.org/tools/install). You'll need `cargo`, the Rust package manager.
- **Git:** Ensure you have Git installed.

## 2. Setup and First Run

1.  **Clone the Repository:**
    Open your terminal or command prompt and run:

    ```bash
    git clone [https://github.com/yashjagtap23/cs128Hproject.git](https://github.com/yashjagtap23/cs128Hproject.git)
    cd cs128Hproject/coffee_chat
    ```

2.  **Build and Run:**
    Inside the `coffee_chat` directory, build and run the application using Cargo:
    ```bash
    cargo run
    ```
    The first time you run this, Cargo will download and compile all necessary dependencies. This might take a few minutes. Subsequent runs will be much faster.

## 3. Configuration

The application requires configuration for Google Calendar API access and SMTP (email sending).

### 3.1. Google Calendar API (`credentials.json`)

To allow the application to access your Google Calendar and find free slots, you need to set up OAuth 2.0 credentials.

1.  **Google Cloud Console:**

    - Go to the [Google Cloud Console](https://console.cloud.google.com/).
    - Create a new project or select an existing one.
    - **Enable the Google Calendar API:**
      - In the navigation menu, go to "APIs & Services" > "Library".
      - Search for "Google Calendar API" and enable it for your project.
    - **Create OAuth 2.0 Credentials:**
      - Go to "APIs & Services" > "Credentials".
      - Click "+ CREATE CREDENTIALS" and choose "OAuth client ID".
      - If prompted, configure the "OAuth consent screen" first:
        - Choose "External" (or "Internal" if you are part of a Google Workspace organization and only you/your org will use it).
        - Fill in the required fields (App name, User support email, Developer contact information).
        - For Scopes, you don't need to add any here; the application will request them.
        - Add your email address as a "Test user" if you are in "External" mode and the app is in testing phase.
        - Save and continue.
      - Back on the "Credentials" page, click "+ CREATE CREDENTIALS" > "OAuth client ID" again.
      - Select "Desktop app" as the Application type.
      - Give it a name (e.g., "CoffeeChatApp Credentials").
      - Click "Create".
    - **Download JSON:**
      - A pop-up will show your Client ID and Client Secret. **Download the JSON file** provided (usually a button like `📥 DOWNLOAD JSON`).
      - This file contains your `client_id`, `client_secret`, `auth_uri`, `token_uri`, etc.

2.  **Place `credentials.json`:**
    - Rename the downloaded JSON file to `credentials.json`.
    - Place this `credentials.json` file in the **root directory** of the `coffee_chat` project (i.e., at the same level as `Cargo.toml` and the `src` folder). The application expects to find it here by default (as specified by `app.credentials_path`).

### 3.2. SMTP Settings (for Sending Emails)

The application uses SMTP to send email invitations. You'll need to configure these settings within the app's UI. The password you enter will be stored securely in your operating system's keychain.

**Recommended: Using Gmail with an App Password**

If you use Gmail and have 2-Step Verification enabled on your Google Account, you cannot use your regular Google password directly in most third-party applications for security reasons. Instead, you need to generate an "App Password".

1.  **Enable 2-Step Verification:** If you haven't already, enable 2-Step Verification for your Google Account.
2.  **Generate an App Password:**
    - Go to your Google Account settings: [https://myaccount.google.com/](https://myaccount.google.com/)
    - Navigate to "Security".
    - Under "Signing in to Google" (or "How you sign in to Google"), find "2-Step Verification". You may need to sign in again.
    - Scroll down to the bottom and click on "App passwords".
    - You might be prompted to sign in again.
    - Under "Select app", choose "Mail".
    - Under "Select device", choose "Other (Custom name)".
    - Enter a name (e.g., "CoffeeChatAutomator") and click "GENERATE".
    - Google will display a **16-character App Password**. **Copy this password immediately.** You won't be able to see it again. This is the password you will use in the application's SMTP settings.

**Alternative SMTP Providers:**

If you are using a different email provider (e.g., Outlook, a custom SMTP server), you'll need their specific SMTP server details:

- SMTP Host (e.g., `smtp.office365.com`)
- SMTP Port (e.g., `587` for TLS, `465` for SSL)
- Your SMTP Username (usually your full email address)
- Your SMTP Password (your regular email password or an app-specific password if required by your provider)

### 3.3. Initial `config.toml` (Optional Defaults)

The application can load initial default values from a `config.toml` file located in the project root. This is useful for setting up some base configuration, but user changes made in the UI and saved state will take precedence.

Create a file named `config.toml` in the `coffee_chat` project root with content like this (adjust as needed):

```toml
[smtp]
host = "smtp.gmail.com"
port = 587
user = "your_email@gmail.com"
# Password is NOT set here anymore; it's managed by the UI and OS keychain.
from_email = "your_email@gmail.com"

[sender]
name = "Your Name"
template_path = "email_template.txt" # Path relative to project root

[[recipients]]
name = "Ada Lovelace"
email = "ada@example.com"

[[recipients]]
name = "Charles Babbage"
email = "charles@example.com"
```

### 3.4. Email Template File (`email_template.txt`)

The content of this file is used to populate the "Email Subject" and "Email Body" fields in the UI **only if no saved application state is found for these fields** (typically on the very first run or if `app_state.json` is missing/corrupted and the corresponding fields aren't set by `config.toml`).

Create a file named `email_template.txt` (or the path specified in `config.toml` which defaults to `email_template.txt` if `config.toml` doesn't exist or specify it) in the project root (where you run `cargo run`).

The format of `email_template.txt` is:

```text
Subject: Coffee Chat Invitation - {{ sender_name }}
---
Hi {{ recipient_name }},

I hope this email finds you well.

I'm {{sender_name}}, and I'd love to connect for a brief coffee chat to discuss [mention a topic or just general connection, e.g., your work on X, our shared interests in Y].

Would you be available sometime in the near future? Here are some times that work for me:
{% for time in availabilities %}
- {{ time }}
{% endfor %}


Please let me know if any of these times work for you, or feel free to suggest an alternative that suits you better.

Looking forward to connecting!

Best regards,
{{ sender_name }}
```

- **`Subject: ...`**: The very first line, starting exactly with "Subject:", sets the default email subject.
- **`---`**: A line containing exactly three hyphens acts as a separator between the subject and the body.
- **Below `---`**: The rest of the content is the default email body.
- **Placeholders (Case-Sensitive):**
  - `{{recipient_name}}`: Will be replaced with the name of the recipient when the email is sent.
  - `{{sender_name}}`: Will be replaced with your "Sender Name" as configured in the SMTP Settings section of the UI.
  - `availabilities`: This is the key in the template context that holds a list (a `Vec<String>`) of your fetched calendar slots.
  - `{% for time in availabilities %}`: This is a Tera template loop. It iterates over each string in the `availabilities` list.
  - `{{ time }}`: Inside the loop, `time` (or any variable name you choose in the `for` loop) will be replaced with an individual availability string (e.g., "Monday May 12: 2pm-3:30pm").

## 4. Using the Application

Once you run `cargo run` and the application window appears:

**Main UI Sections:**

- **Left Panel (Central Area): "Email Message & Calendar"**
  - This is where you compose your email, manage your Google Calendar connection, adjust calendar-related settings, and fetch available time slots.
- **Right Panel: "Recipients" and "SMTP Settings"**
  - The top part allows you to add and manage the list of people you want to invite.
  - The bottom part is for configuring your email sending (SMTP) details.
- **Bottom Bar:**
  - Displays status messages about the application's operations (e.g., "Loaded previous session," "Email sent successfully," error messages).
  - Shows spinners (loading indicators) during background tasks like connecting to the calendar, fetching slots, or sending emails.

**Typical Workflow & Button Functions:**

1.  **First Run - Google Calendar Authorization:**

    - In the "Email Message & Calendar" section (left panel), click the **"📅 Connect Google Calendar"** button.
      - **Function:** Initiates the OAuth 2.0 flow to grant the application permission to access your Google Calendar.
    - Your default web browser will open, guiding you through the Google Account login and authorization process. You'll need to grant the requested permissions (typically to view your calendars and events).
    - After successful authorization, your browser will likely show a success message or redirect to a local address. The application automatically captures the necessary authorization token.
    - The button text in the app should change to "✅ Calendar Connected", and the status label next to it will confirm the connection.
    - A `tokencache.json` file will be created in the project root (where you run `cargo run`). This file stores your OAuth token, so you generally won't need to re-authorize every time you start the app unless the token expires, is revoked, or the file is deleted.

2.  **Configure SMTP Settings (Right Panel):**

    - **Host:** Text field. Enter your SMTP server address (e.g., `smtp.gmail.com`).
    - **Port:** Text field. Enter the SMTP port (e.g., `587` for TLS).
    - **Username:** Text field. Enter your SMTP username (e.g., `your_email@gmail.com`).
      - **Behavior:** If you change the username after a password has been saved for a previous username, the displayed password (if any) and the internally held password will clear. The app will then attempt to load a password from your OS keychain for the _new_ username you just entered.
    - **Password:**
      - Text field (input appears as dots: ●●●●●●●●). Type your SMTP password here (e.g., the 16-character Gmail App Password if using Gmail).
      - **"Save to Keychain" button:**
        - **Function:** Securely stores the password you typed into the password field in your operating system's keychain (or credential manager). The password is associated with the entered SMTP Username and the application's unique service name.
        - **When to use:** After typing your password, click this to save it for future sessions.
        - **Note:** For this button to be enabled, both the "Username" field and the password input field must contain text.
    - **From Email:** Text field. Enter the email address you want the emails to appear to be sent from (this is usually the same as your SMTP Username).
    - **Sender Name:** Text field. Enter your name as you want it to appear as the sender of the email.

3.  **Edit Email Message (Left Panel - "Email Message & Calendar" section):**

    - **Subject:** Single-line text field.
      - **Function:** Allows you to edit the subject line of your invitation email.
      - **Content:** It will be pre-filled from your last saved session, or from the `Subject:` line in `email_template.txt` on the very first run (if no saved state exists for the subject). You can use the placeholders `{{ recipient_name }}`, `{{ sender_name }}`, and `{{ availabilities }}`.
    - **Body:** Multi-line text area.
      - **Function:** Allows you to edit the main content/body of your email.
      - **Content:** Pre-filled from your last session, or from the content below `---` in `email_template.txt` on the very first run. It also supports the placeholders `{{ recipient_name }}`, `{{ sender_name }}`, and `{{ availabilities }}`.
    - Edits made to the Subject and Body in the UI are saved to `app_state.json` when you close the app and will be loaded next time.

4.  **Configure Calendar Settings (Left Panel - Collapsible Section):**

    - Click on the **"Calendar Settings"** header to expand/collapse this section.
    - **Event Buffer:**
      - Slider and an adjacent text box (allows typing or using up/down arrows).
      - **Function:** Sets the buffer time (in minutes) that the application should consider around your existing calendar events. Free slots will not be proposed if they fall within this buffer period before or after an existing event.
      - **Range:** 0 to 120 minutes.
    - **Daily Availability:**
      - Double-ended slider and two adjacent text boxes ("From" and "To").
      - **Function:** Defines the general time window (e.g., 9:00 to 17:00 for 9 AM to 5 PM) within each day for which you want the application to find and propose coffee chat slots. Slots outside this window will be filtered out.
      - **Range:** 0:00 (midnight) to 23:00 (11 PM).

5.  **Fetch Available Slots (Left Panel):**

    - **"🔄 Fetch Slots" button:**
      - **Function:** When clicked, the application queries your connected Google Calendar for periods of free time. It considers events within the next 14 days, applies your "Event Buffer" and "Daily Availability" settings, and filters out very short slots.
      - **Enabled:** Only active if the calendar is connected and the app isn't already fetching.
    - **"Available Slots:" box:** A scrollable list area below the button.
      - **Function:** Displays the calculated available time slots, formatted for readability. These are the slots that will be inserted into the `{{availabilities}}` placeholder in your email.

6.  **Manage Recipients (Right Panel - "Recipients" section):**

    - **Name:** Text field. Enter the first name or full name of the person you want to invite.
    - **Email:** Text field. Enter their email address.
    - **"Add" button:**
      - **Function:** Adds the entered Name and Email to the "Current List:" below. Both fields must be filled, and the email should contain an "@" symbol.
    - **"Current List:" box:** A scrollable list area.
      - **Function:** Displays the names and email addresses of all recipients you've added for the current batch of invitations.
    - **"X" button (next to each recipient):**
      - **Function:** Removes that specific recipient from the "Current List".

7.  **Send Invitations (Left Panel - Bottom):**
    - **"🚀 Send Invitations" button:**
      - **Function:** This is the main action button. When clicked, the application attempts to send the composed email (with placeholders filled) to every recipient in the "Current List", using the configured SMTP settings.
      - **Enabled:** Only active if the app is not already sending emails, not connecting to the calendar, not fetching slots, and the initial configuration has been processed.
