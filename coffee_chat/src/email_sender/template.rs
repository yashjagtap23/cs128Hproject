use std::fs;
use std::path::Path;
use tera::{Context, Error as TeraError, Tera}; // Templating engine
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Failed to read template file '{path}': {source}")]
    ReadError {
        path: String,
        source: std::io::Error,
    },
    #[error("Failed to parse template '{name}': {source}")] // Changed path to name for clarity
    ParseError { name: String, source: TeraError },
    #[error("Failed to render template: {0}")]
    RenderError(#[from] TeraError),
    #[error("Template format error: Missing 'Subject:' line or '---' separator")]
    FormatError,
}

/// Represents the parsed email template content.
// Making fields pub(crate) allows access within the crate but not outside.
// Alternatively, keep them private and use constructors/methods.
pub struct EmailTemplate {
    pub subject_template: String,
    pub body_template: String,
    // Keep these private, managed by constructors
    tera: Tera,
    template_name: String,
}

impl EmailTemplate {
    /// Loads and parses the email template from a file.
    /// Expects format:
    /// Subject: <subject template>
    /// ---
    /// <body template>
    pub fn load(template_path: &Path) -> Result<Self, TemplateError> {
        let path_str = template_path.to_string_lossy().to_string();
        let content = fs::read_to_string(template_path).map_err(|e| TemplateError::ReadError {
            path: path_str.clone(),
            source: e,
        })?;

        // Split subject and body
        let mut lines = content.lines();
        let subject_line = lines.next().ok_or(TemplateError::FormatError)?;
        let separator = lines.next().ok_or(TemplateError::FormatError)?;

        if !subject_line.starts_with("Subject:") || separator != "---" {
            return Err(TemplateError::FormatError);
        }

        let subject_template = subject_line
            .trim_start_matches("Subject:")
            .trim()
            .to_string();
        let body_template = lines.collect::<Vec<&str>>().join("\n");

        // Use the new constructor internally
        Self::from_content(&subject_template, &body_template, "file_template")
    }

    /// --- NEW CONSTRUCTOR ---
    /// Creates an EmailTemplate directly from subject and body strings.
    /// Useful for creating templates from UI input.
    pub fn from_content(subject: &str, body: &str, base_name: &str) -> Result<Self, TemplateError> {
        let mut tera = Tera::default();
        // Ensure unique names for Tera internal registry
        let subject_template_name = format!("{}_subject", base_name);
        let body_template_name = format!("{}_body", base_name);

        tera.add_raw_templates(vec![
            (&subject_template_name, subject),
            (&body_template_name, body),
        ])
        .map_err(|e| TemplateError::ParseError {
            name: base_name.to_string(), // Use base_name for error reporting
            source: e,
        })?;

        Ok(EmailTemplate {
            subject_template: subject.to_string(),
            body_template: body.to_string(),
            tera,
            // Store the base name used for rendering lookups
            template_name: base_name.to_string(),
        })
    }

    /// Renders the subject and body using the provided context.
    pub fn render(
        &self,
        recipient_name: &str,
        sender_name: &str,
        availabilities: &[String], // Assuming availabilities are strings
    ) -> Result<(String, String), TemplateError> {
        let mut context = Context::new();
        context.insert("recipient_name", recipient_name);
        context.insert("sender_name", sender_name);
        context.insert("availabilities", availabilities);

        // Use the stored template_name base to construct the full names for rendering
        let subject = self
            .tera
            .render(&format!("{}_subject", self.template_name), &context)?;
        let body = self
            .tera
            .render(&format!("{}_body", self.template_name), &context)?;

        Ok((subject, body))
    }
}
