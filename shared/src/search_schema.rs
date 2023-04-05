use tantivy::schema::Field;
use tantivy::schema::{Schema};
use tantivy::schema::{STORED, TEXT};

#[derive(Clone)]
pub struct SearchSchema {
    pub title: Field,
    pub path: Field,
    pub page: Field,
    pub body: Field,
    pub schema: Schema
}

impl SearchSchema {
    pub fn new(title: Field, path: Field, page: Field, body: Field, schema: Schema) -> Self {
        Self {title, path, page, body, schema}
    }

    pub fn default() -> Self {
        let mut schema_builder = Schema::builder();
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let page = schema_builder.add_u64_field("page", STORED);
        let body = schema_builder.add_text_field("body", TEXT);
        let schema = schema_builder.build();
        Self { title, path, page, body, schema }
    }


    pub fn default_fields(&self) -> Vec<Field> {
        vec![self.title, self.body]
    }
}
