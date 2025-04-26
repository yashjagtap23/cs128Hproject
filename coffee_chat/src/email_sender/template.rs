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
    #[error("Failed to parse template '{path}': {source}")]
    ParseError { path: String, source: TeraError },
    #[error("Failed to render template: {0}")]
    RenderError(#[from] TeraError),
    #[error("Template format error: Missing 'Subject:' line or '---' separator")]
    FormatError,
}

/// Represents the parsed email template content.
pub struct EmailTemplate {
    pub subject_template: String,
    pub body_template: String,
    tera: Tera,            // Keep Tera instance for rendering
    template_name: String, // Name used within Tera
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

        // Initialize Tera and add the templates
        let mut tera = Tera::default();
        let template_name = "email_content"; // Unique name for Tera
        tera.add_raw_templates(vec![
            (&(template_name.to_string() + "_subject"), &subject_template),
            (&(template_name.to_string() + "_body"), &body_template),
        ])
        .map_err(|e| TemplateError::ParseError {
            path: path_str,
            source: e,
        })?;

        Ok(EmailTemplate {
            subject_template, // Keep original for reference if needed
            body_template,    // Keep original for reference if needed
            tera,
            template_name: template_name.to_string(),
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

        let subject = self
            .tera
            .render(&(self.template_name.clone() + "_subject"), &context)?;
        let body = self
            .tera
            .render(&(self.template_name.clone() + "_body"), &context)?;

        Ok((subject, body))
    }
}
