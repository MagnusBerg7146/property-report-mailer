use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct MaintenanceRequest {
    pub unit: String,
    pub summary: String,
    pub days_open: u16,
}

#[derive(Debug, Clone)]
pub struct TenantDocument {
    pub unit: String,
    pub name: String,
    pub days_until_expiry: i16,
}

#[derive(Debug, Clone)]
pub struct InspectionReminder {
    pub unit: String,
    pub due_in_days: u16,
}

#[derive(Debug, Clone)]
pub struct PropertySnapshot {
    pub property_name: String,
    pub maintenance: Vec<MaintenanceRequest>,
    pub documents: Vec<TenantDocument>,
    pub inspections: Vec<InspectionReminder>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeliveryDecision {
    Send { attention_items: usize },
    Skip,
}

impl PropertySnapshot {
    pub fn delivery_decision(&self) -> DeliveryDecision {
        let attention_items = self
            .maintenance
            .iter()
            .filter(|item| item.days_open >= 7)
            .count()
            + self
                .documents
                .iter()
                .filter(|item| item.days_until_expiry <= 30)
                .count()
            + self
                .inspections
                .iter()
                .filter(|item| item.due_in_days <= 14)
                .count();

        if attention_items == 0 {
            DeliveryDecision::Skip
        } else {
            DeliveryDecision::Send { attention_items }
        }
    }

    pub fn pdf_bytes(&self) -> Vec<u8> {
        let mut lines = vec![
            format!("Property operations report: {}", self.property_name),
            format!("Open maintenance requests: {}", self.maintenance.len()),
        ];
        lines.extend(self.maintenance.iter().map(|item| {
            format!(
                "Maintenance | {} | {} | {} days",
                item.unit, item.summary, item.days_open
            )
        }));
        lines.extend(self.documents.iter().map(|item| {
            format!(
                "Document | {} | {} | expires in {} days",
                item.unit, item.name, item.days_until_expiry
            )
        }));
        lines.extend(self.inspections.iter().map(|item| {
            format!(
                "Inspection | {} | due in {} days",
                item.unit, item.due_in_days
            )
        }));
        minimal_pdf(&lines)
    }

    pub fn email_request(&self, to: String) -> EmailRequest {
        let pdf = STANDARD.encode(self.pdf_bytes());
        EmailRequest {
            to,
            subject: format!("{} property action report", self.property_name),
            html: format!(
                "<p>The property action report is ready.</p><p><a download=\"property-report.pdf\" href=\"data:application/pdf;base64,{pdf}\">Download the PDF report</a></p>"
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EmailRequest {
    pub to: String,
    pub subject: String,
    pub html: String,
}

fn pdf_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            '\\' => "\\\\".to_string(),
            ch if ch.is_ascii_graphic() || ch == ' ' => ch.to_string(),
            _ => "?".to_string(),
        })
        .collect()
}

fn minimal_pdf(lines: &[String]) -> Vec<u8> {
    let mut stream = String::from("BT /F1 11 Tf 50 760 Td 14 TL ");
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            stream.push_str("T* ");
        }
        stream.push_str(&format!("({}) Tj ", pdf_text(line)));
    }
    stream.push_str("ET");

    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_only_when_an_item_needs_attention() {
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
                days_until_expiry: 45,
            }],
            inspections: vec![],
        };

        assert_eq!(
            snapshot.delivery_decision(),
            DeliveryDecision::Send { attention_items: 1 }
        );
        assert!(snapshot.pdf_bytes().starts_with(b"%PDF-1.4"));
    }
}
