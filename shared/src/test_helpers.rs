use lopdf::dictionary;
use std::fs::{create_dir_all, remove_dir_all, remove_file};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream};

/// Generate a fake pdf document.
/// Source: [github.com/J-F-Liu/lopdf](https://github.com/J-F-Liu/lopdf#example-code)
pub fn generate_fake_pdf_document(page_texts: Vec<String>) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    let mut page_ids = vec![];
    for text in page_texts {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 48.into()]),
                Operation::new("Td", vec![100.into(), 600.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        page_ids.push(page_id);
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.clone()
            .into_iter()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
        "Count" => page_ids.len() as u32,
    };

    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);

    doc
}

pub fn save_fake_pdf_document(directory: &str, file_name: &str, page_texts: Vec<String>) {
    create_dir_all(directory).unwrap_or_else(|_| panic!("Failed to create directory: {directory}"));
    let mut doc = generate_fake_pdf_document(page_texts);
    doc.save(format!("{directory}/{file_name}"))
        .unwrap_or_else(|_| panic!("Failed to save test document: {file_name}"));
}

pub fn cleanup_dir_and_file(directory: &str, file_name: &str) {
    remove_dir_all(directory).unwrap_or_else(|_| panic!("Failed to remove directory: {directory}"));
    // remove if exists, drop result
    let _ = remove_file(file_name);
}
