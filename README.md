# cs128Hproject

# Coffee Chat Email Bot

## Group Name
Group 17

## Group Members
- Yash Jagtap
- Michael Feng
- Seoeun (Blaire) Jung

## Project Introduction

[We pivoted to automating mass email sends for consultants due to linkedin API issues.]
Thus, this old outline is a bit outdated however most of info is correct aside from Linkedin stuff. 

The Coffee Chat Email Bot is designed to streamline professional networking by automating the process of sending personalized coffee chat invitations. This tool tailors emails by leveraging data from LinkedIn profiles alongside the user’s calendar availability. By handling the collection, integration, and scheduling automatically, the bot reduces administrative workload while creating engaging and personalized outreach messages.

**Goals and Objectives:**
- Personalized Outreach: Automatically extract relevant details (name, job title, company, etc.) from LinkedIn profiles to craft custom messages.
- Schedule Coordination: Integrate with calendar services (Google Calendar, Outlook, etc.) to propose convenient meeting times.
- Enhanced Efficiency: Reduce manual effort in networking and scheduling by automating the email drafting and sending process.
- High-Quality User Engagement: Improve response rates by offering well-timed and personalized meeting invitations.

**Why This Project?**  
We chose this project because it addresses a common challenge in professional networking—time management and personalization. The integration of multiple APIs in a systems programming language like Rust provides an opportunity to delve into asynchronous programming, efficient email delivery, all while building a tool that has real-world utility.

## Technical Overview

### Major Components
1. LinkedIn Data Integration
   - Function: Extract critical profile details such as name, current role, company, and interests.
   - Implementation: Use LinkedIn’s official API with OAuth for secure data retrieval.

2. Calendar Integration
   - Function: Determine the user’s available time slots to suggest meeting times.
   - Implementation: Connect with major calendar services using their APIs and employ the `chrono` crate (Rust library used for date and time handling) for date and time management.

3. Email Template Generation
   - Function: Create dynamic, personalized email content.
   - Implementation: Utilize a Rust templating engine such as Askama or Tera (templating engines used in Rust for generating text based on templates, Tera is more user-friendly) to dynamically insert retrieved data into predefined email templates.

4. Email Dispatch
   - Function: Send the personalized emails.
   - Implementation: Employ the Lettre crate to construct and send emails over SMTP (Internet standard communication protocol used for sending email messages between servers), handling potential errors and retries.

5. Asynchronous Workflow
   - Function: Manage simultaneous API calls for LinkedIn, calendar data retrieval, and email dispatch.
   - Implementation: Leverage the Tokio async (using Tokio’s asynchronous programming model to handle multiple operations (like API calls or email sending) concurrently without blocking the thread) runtime to ensure non-blocking operations throughout the application.

### Roadmap and Checkpoints
- Checkpoint 1 (Weeks 1-2):  
  - Initialize the repository and set up the Rust project structure.
  - Draft the design document and define project requirements.
  - Conduct preliminary research on integrating with the LinkedIn and calendar APIs.

- Checkpoint 2 (Weeks 3-4): 
  - Implement a basic LinkedIn API client to fetch profile data (starting with placeholder data).
  - Develop initial calendar querying functionality to retrieve free time slots.
  - Create basic email templates using Askama/Tera.

- Checkpoint 3 (Weeks 5-6): 
  - Integrate LinkedIn data and calendar availability into the email generation process.
  - Implement asynchronous operations using Tokio for API calls and email sending.
  - Establish error handling and logging mechanisms.

- Checkpoint 4 (Final Weeks):  
  - Complete end-to-end integration and perform comprehensive testing.
  - Refine email content for natural language quality and professional tone.
  - Optimize performance and security; finalize documentation and prepare the project presentation.

## Possible Challenges
- API Authentication: Handling OAuth across multiple services (LinkedIn, Google Calendar, etc.) and ensuring secure storage of tokens.
- Asynchronous Data Handling: Effectively coordinating concurrent API calls using Tokio.
- Dynamic Content Generation: Generating natural, contextually appropriate email content based on diverse data sources.
- Error Handling: Building robust error handling for API limitations, rate limits, or intermittent network issues.
- Privacy & Security: Safeguarding sensitive user data and complying with privacy standards such as GDPR.

## References
- LinkedIn API Documentation: [LinkedIn API](https://docs.microsoft.com/en-us/linkedin/)
- Google Calendar API Documentation: [Google Calendar API](https://developers.google.com/calendar)
- Askama (Rust Templating): [Askama GitHub](https://github.com/djc/askama)
- Lettre (Email Sending in Rust): [Lettre GitHub](https://github.com/lettre/lettre)
- Rust Asynchronous Programming: [Tokio](https://tokio.rs/)

