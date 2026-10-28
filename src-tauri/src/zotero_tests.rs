//! Import from a simulated Zotero data folder (same tables and columns Zotero uses).

use tempfile::tempdir;

use super::*;
use crate::library::tests::create_pdf;

const SCHEMA: &str = "
CREATE TABLE itemTypes (itemTypeID INTEGER PRIMARY KEY, typeName TEXT);
CREATE TABLE items (itemID INTEGER PRIMARY KEY, itemTypeID INT, libraryID INT, key TEXT);
CREATE TABLE deletedItems (itemID INTEGER PRIMARY KEY);
CREATE TABLE fields (fieldID INTEGER PRIMARY KEY, fieldName TEXT);
CREATE TABLE itemDataValues (valueID INTEGER PRIMARY KEY, value);
CREATE TABLE itemData (itemID INT, fieldID INT, valueID INT);
CREATE TABLE creators (creatorID INTEGER PRIMARY KEY, firstName TEXT, lastName TEXT, fieldMode INT);
CREATE TABLE itemCreators (itemID INT, creatorID INT, creatorTypeID INT, orderIndex INT);
CREATE TABLE tags (tagID INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE itemTags (itemID INT, tagID INT, type INT);
CREATE TABLE collections (collectionID INTEGER PRIMARY KEY, collectionName TEXT, parentCollectionID INT, libraryID INT, key TEXT);
CREATE TABLE collectionItems (collectionID INT, itemID INT, orderIndex INT);
CREATE TABLE itemAttachments (itemID INTEGER PRIMARY KEY, parentItemID INT, linkMode INT, contentType TEXT, path TEXT);

INSERT INTO itemTypes VALUES (2, 'journalArticle'), (3, 'attachment'), (4, 'note'), (5, 'conferencePaper');
INSERT INTO fields VALUES (1, 'title'), (2, 'date'), (3, 'DOI'), (4, 'abstractNote'), (5, 'publicationTitle'), (6, 'proceedingsTitle');
INSERT INTO items VALUES
  (1, 2, 1, 'PAPERONE'), (2, 3, 1, 'ATTACHA1'), (3, 5, 1, 'CONFTWO1'),
  (4, 2, 1, 'TRASHED1'), (5, 4, 1, 'NOTEONE1'), (6, 2, 1, 'KNOWNDOI');
INSERT INTO deletedItems VALUES (4);
INSERT INTO itemDataValues VALUES
  (1, 'Murals with few labels'), (2, '2024-03-00 March 2024'), (3, 'https://doi.org/10.1000/Mural.1'),
  (4, 'Semi-supervised segmentation.'), (5, 'Heritage Science'),
  (6, 'Continual learning on photographs'), (7, '2023'), (8, 'Proc. Vision'),
  (9, 'Deleted paper'), (10, 'Already in the library'), (11, '10.1000/known.9');
INSERT INTO itemData VALUES
  (1, 1, 1), (1, 2, 2), (1, 3, 3), (1, 4, 4), (1, 5, 5),
  (3, 1, 6), (3, 2, 7), (3, 6, 8),
  (4, 1, 9),
  (6, 1, 10), (6, 3, 11);
INSERT INTO creators VALUES (1, 'Ana', 'Researcher', 0), (2, NULL, 'Heritage Lab', 1), (3, 'Luis', 'Scientist', 0);
INSERT INTO itemCreators VALUES (1, 1, 1, 0), (1, 2, 1, 1), (3, 3, 1, 0);
INSERT INTO tags VALUES (1, 'murals'), (2, 'segmentation');
INSERT INTO itemTags VALUES (1, 2, 0), (1, 1, 0);
INSERT INTO collections VALUES (1, 'Tesis', NULL, 1, 'COLL0001'), (2, 'Capítulo 2', 1, 1, 'COLL0002');
INSERT INTO collectionItems VALUES (2, 1, 0), (1, 3, 0);
INSERT INTO itemAttachments VALUES (2, 1, 0, 'application/pdf', 'storage:murals.pdf');
";

#[test]
fn imports_items_pdfs_and_collections_once() {
    let directory = tempdir().unwrap();
    let zotero = directory.path().join("Zotero");
    fs::create_dir_all(zotero.join("storage").join("ATTACHA1")).unwrap();
    create_pdf(&zotero.join("storage").join("ATTACHA1").join("murals.pdf"));
    Connection::open(zotero.join("zotero.sqlite"))
        .unwrap()
        .execute_batch(SCHEMA)
        .unwrap();

    let database =
        Database::initialize(directory.path().join("data").join("researchpilot.sqlite3")).unwrap();
    let known = directory.path().join("known.ris");
    fs::write(
        &known,
        "TY  - JOUR\nTI  - Already in the library\nDO  - 10.1000/known.9\nER  - \n",
    )
    .unwrap();
    database
        .import_reference_files(vec![known.to_string_lossy().into()])
        .unwrap();

    let summary = database.import_zotero(&zotero, true).unwrap();
    assert_eq!(
        summary.items, 3,
        "notes, attachments and trashed items are skipped"
    );
    assert_eq!(
        (
            summary.imported,
            summary.linked_existing,
            summary.pdfs_attached
        ),
        (2, 1, 1)
    );
    assert_eq!(summary.collections, 2);
    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
    assert_eq!(summary.new_pdf_articles.len(), 1);

    let articles = database.library_articles(None).unwrap();
    assert_eq!(articles.len(), 3);
    let murals = articles
        .iter()
        .find(|a| a.title == "Murals with few labels")
        .unwrap();
    assert_eq!(murals.publication_year, Some(2024));
    assert_eq!(murals.doi.as_deref(), Some("10.1000/mural.1"));
    assert_eq!(murals.authors, vec!["Researcher, Ana", "Heritage Lab"]);
    assert_eq!(murals.keywords, vec!["murals", "segmentation"]);
    assert_eq!(murals.venue.as_deref(), Some("Heritage Science"));
    assert!(
        murals.document_path.is_some(),
        "the PDF went through the importer"
    );
    let conference = articles
        .iter()
        .find(|a| a.title == "Continual learning on photographs")
        .unwrap();
    assert_eq!(conference.venue.as_deref(), Some("Proc. Vision"));
    assert!(conference.document_path.is_none());

    let collections = database.library_collections().unwrap();
    let mut names: Vec<_> = collections.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Tesis", "Tesis / Capítulo 2"]);

    // The source file is never modified and a second import adds nothing.
    let again = database.import_zotero(&zotero, true).unwrap();
    assert_eq!((again.imported, again.already_linked), (0, 3));
    assert_eq!(database.library_articles(None).unwrap().len(), 3);
    assert!(
        fs::read_dir(directory.path().join("data"))
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("zotero-")
            }),
        "the private copy is removed"
    );
}

#[test]
fn rejects_folders_without_a_zotero_database() {
    let directory = tempdir().unwrap();
    let database = Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
    let error = database
        .import_zotero(directory.path(), true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("zotero.sqlite"), "{error}");

    let fake = directory.path().join("fake");
    fs::create_dir_all(&fake).unwrap();
    Connection::open(fake.join("zotero.sqlite"))
        .unwrap()
        .execute_batch("CREATE TABLE something (id INTEGER)")
        .unwrap();
    let error = database.import_zotero(&fake, true).unwrap_err().to_string();
    assert!(error.contains("estructura"), "{error}");
}
