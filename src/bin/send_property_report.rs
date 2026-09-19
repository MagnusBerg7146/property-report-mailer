use property_report_mailer::{
    infrai_email::{EmailError, InfraiEmailClient},
    property_report::{
        DeliveryDecision, InspectionReminder, MaintenanceRequest, PropertySnapshot, TenantDocument,
    },
};
use std::env;
use thiserror::Error;

#[derive(Debug, Error)]
enum RunError {
    #[error("REPORT_RECIPIENT is required")]
    MissingRecipient,
    #[error(transparent)]
    Email(#[from] EmailError),
}

#[tokio::main]
async fn main() -> Result<(), RunError> {
    let recipient = env::var("REPORT_RECIPIENT").map_err(|_| RunError::MissingRecipient)?;
    let snapshot = PropertySnapshot {
        property_name: "Harbor Court".into(),
        maintenance: vec![MaintenanceRequest {
            unit: "4B".into(),
            summary: "Heating inspection".into(),
            days_open: 8,
        }],
        documents: vec![TenantDocument {
            unit: "2A".into(),
            name: "Insurance certificate".into(),
            days_until_expiry: 21,
        }],
        inspections: vec![InspectionReminder {
            unit: "1C".into(),
            due_in_days: 10,
        }],
    };

    match snapshot.delivery_decision() {
        DeliveryDecision::Skip => println!("No action report due."),
        DeliveryDecision::Send { attention_items } => {
            let request = snapshot.email_request(recipient);
            let message_id = InfraiEmailClient::from_env()?
                .send(&request, "harbor-court-action-report-2026-09-18")
                .await?;
            println!("Sent {attention_items} action items; message_id={message_id}");
        }
    }
    Ok(())
}
