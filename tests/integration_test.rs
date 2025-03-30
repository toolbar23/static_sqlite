use futures::pin_mut;
use futures::StreamExt;
use static_sqlite::{sql, FirstRow, Result, Sqlite};
#[tokio::test]
async fn option_type_works() -> Result<()> {
    sql! {
        let migrate = r#"
            create table Row (
                txt text
            )
        "#;

        let insert_row = r#"
            insert into Row (txt) values (:txt) returning *
        "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    let _k = migrate(&db).await?;
    let txt = Some("txt");
    let row = insert_row(&db, txt).await?.first_row()?;

    assert_eq!(row.txt, Some("txt".into()));

    Ok(())
}

#[tokio::test]
async fn stream_works() -> Result<()> {
    sql! {
        let migrate = r#"
            create table Row (
                txt text
            )
        "#;

        let insert_row = r#"
            insert into Row (txt) values (:txt) returning *
        "#;

        let select_rows = r#"
            select * from Row
        "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    migrate(&db).await?;

    insert_row(&db, Some("test1")).await?.first_row()?;
    insert_row(&db, Some("test2")).await?.first_row()?;
    insert_row(&db, Some("test3")).await?.first_row()?;
    insert_row(&db, Some("test4")).await?.first_row()?;

    let f = select_rows_stream(&db).await?;

    pin_mut!(f);

    assert_eq!(f.next().await.unwrap().unwrap().txt, Some("test1".into()));
    assert_eq!(f.next().await.unwrap().unwrap().txt, Some("test2".into()));
    assert_eq!(f.next().await.unwrap().unwrap().txt, Some("test3".into()));
    assert_eq!(f.next().await.unwrap().unwrap().txt, Some("test4".into()));

    Ok(())
}

#[tokio::test]
async fn it_works() -> Result<()> {
    sql! {
        let migrations = r#"
            create table User (
                id integer primary key,
                email text not null unique
            );

            create table Row (
                not_null_text text not null,
                not_null_integer integer not null,
                not_null_real real not null,
                not_null_blob blob not null,
                null_text text,
                null_integer integer,
                null_real real,
                null_blob blob
            );

            alter table Row add column nullable_text text;
            alter table Row add column nullable_integer integer;
            alter table Row add column nullable_real real;
            alter table Row add column nullable_blob blob;
        "#;

        let insert_row = r#"
            insert into Row (
                not_null_text,
                not_null_integer,
                not_null_real,
                not_null_blob,
                null_text,
                null_integer,
                null_real,
                null_blob,
                nullable_text,
                nullable_integer,
                nullable_real,
                nullable_blob
            )
            values (
                :not_null_text,
                :not_null_integer,
                :not_null_real,
                :not_null_blob,
                :null_text,
                :null_integer,
                :null_real,
                :null_blob,
                :nullable_text,
                :nullable_integer,
                :nullable_real,
                :nullable_blob
            )
            returning *
        "#;
    }

    async fn db(path: &str) -> Result<Sqlite> {
        let sqlite = static_sqlite::open(path).await?;
        static_sqlite::execute_all(
            &sqlite,
            r#"
            pragma journal_mode = wal;
            pragma synchronous = normal;
            pragma foreign_keys = on;
            pragma busy_timeout = 5000;
            pragma cache_size = -64000;
            pragma strict = on;
        "#,
        )
        .await?;
        migrations(&sqlite).await?;
        Ok(sqlite)
    }

    let db = db(":memory:").await?;

    let row = insert_row(
        &db,
        "not_null_text",
        1,
        1.0,
        vec![0xBE, 0xEF],
        None::<String>,
        None,
        None,
        None,
        Some("nullable_text"),
        Some(2),
        Some(2.0),
        Some(vec![0xFE, 0xED]),
    )
    .await?
    .first_row()?;

    assert_eq!(
        row,
        InsertRow {
            not_null_text: "not_null_text".into(),
            not_null_integer: 1,
            not_null_real: 1.,
            not_null_blob: vec![0xBE, 0xEF],
            null_text: None,
            null_integer: None,
            null_real: None,
            null_blob: None,
            nullable_text: Some("nullable_text".into()),
            nullable_integer: Some(2),
            nullable_real: Some(2.),
            nullable_blob: Some(vec![0xFE, 0xED]),
        }
    );

    Ok(())
}

#[tokio::test]
async fn readme_works() -> Result<()> {
    sql! {
        let migrate = r#"
            create table User (
                id integer primary key,
                name text unique not null
            );

            alter table User
            add column created_at integer;

            alter table User
            drop column created_at;
        "#;

        let insert_user = r#"
            insert into User (name)
            values (:name)
            returning *
        "#;

    }

    let db = static_sqlite::open(":memory:").await?;
    let _ = migrate(&db).await?;
    let user = insert_user(&db, "swlkr").await?.first_row()?;

    assert_eq!(user.id, 1);
    assert_eq!(user.name, "swlkr");

    Ok(())
}

#[tokio::test]
async fn crud_works() -> Result<()> {
    sql! {
        let migrate = r#"
            create table User (
                id integer primary key,
                name text unique not null
            );
        "#;

        let insert_user = r#"
            insert into User (name)
            values (:name)
            returning *
        "#;

        let update_user = r#"
            update User set name = :name where id = :id returning *
        "#;

        let delete_user = r#"
            delete from User where id = :id
        "#;

        let all_users = r#"
            select id, name from User
        "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    let _ = migrate(&db).await?;
    let user = insert_user(&db, "swlkr").await?.first_row()?;
    assert_eq!(user.id, 1);
    assert_eq!(user.name, "swlkr");

    let users = all_users(&db).await?;
    assert_eq!(users.len(), 1);
    let user = users.first().unwrap();
    assert_eq!(user.id, 1);
    assert_eq!(user.name, "swlkr");

    let user = update_user(&db, "swlkr2", 1).await?.first_row()?;
    assert_eq!(user.id, 1);
    assert_eq!(user.name, "swlkr2");

    delete_user(&db, 1).await?;
    let users = all_users(&db).await?;
    assert_eq!(users.len(), 0);

    Ok(())
}

#[tokio::test]
async fn parameters_that_are_not_in_the_schema_work() -> Result<()> {
    sql! {
        let migrate = r#"
            create table User (
                id integer primary key,
                name text unique not null
            );

            create table Post (
                id integer primary key,
                user_id integer not null references User(id),
                name text unique not null
            );
        "#;

        let insert_user = r#"
            insert into User (name) values (:name) returning *
        "#;

        let insert_post = r#"
            insert into Post (user_id, name) values (:user_id, :name) returning *
        "#;
        let select_posts = r#"
            select * from Post  where id = :id AND id = :id__INTEGER AND name = :id__INTEGER AND name = :name AND :ff__TEXT="sdd"
         "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    let _ = migrate(&db).await?;
    let user1 = insert_user(&db, "user1").await?.first_row()?;
    insert_post(&db, user1.id, "user 1 - post1")
        .await?
        .first_row()?;
    insert_post(&db, user1.id, "user 1 - post2")
        .await?
        .first_row()?;
    let user2 = insert_user(&db, "user2").await?.first_row()?;
    insert_post(&db, user2.id, "user 2 - post1")
        .await?
        .first_row()?;
    insert_post(&db, user2.id, "user 2 - post2")
        .await?
        .first_row()?;

    let posts = select_posts(&db, 1, 2, "Hello", "sdd").await?;
    println!("{:?}", posts);

    Ok(())
}

#[tokio::test]
async fn example_friendshipworks() -> Result<()> {
    use static_sqlite::{self, sql, Result};

    sql! {
        let migrate = r#"
            create table User (
                id integer primary key,
                name text unique not null
            );

            create table Friendship (
                id integer primary key,
                user_id integer not null references User(id),
                friend_id integer not null references User(id)
            );
        "#;

        let insert_user = r#"
            insert into User (name)
            values (:name)
            returning *
        "#;
        let create_friendship = r#"
            insert into Friendship (user_id, friend_id)
            values (:user_id, :friend_id)
            returning *
        "#;
        let get_friendship = r#"
            SELECT
                u1.name as friend1_name__TEXT,
                u2.name as friend2_name__TEXT
            FROM Friendship, User as u1, User as u2
            WHERE Friendship.user_id = u1.id
                  AND Friendship.friend_id = u2.id
                  AND Friendship.id = :friendship_id__INTEGER
        "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    let _ = migrate(&db).await?;
    insert_user(&db, "swlkr").await?;
    insert_user(&db, "toolbar23").await?;
    create_friendship(&db, 1, 2).await?;

    let friends = get_friendship(&db, 1).await?;

    assert_eq!(friends.len(), 1);
    assert_eq!(friends.first().unwrap().friend1_name, "swlkr");
    assert_eq!(friends.first().unwrap().friend2_name, "toolbar23");

    Ok(())
}

#[tokio::test]
async fn duplicate_column_names_in_one_query_work() -> Result<()> {
    sql! {
        let migrate = r#"
        CREATE TABLE IF NOT EXISTS Identifiers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type INTEGER NOT NULL,
            identifier_type TEXT NOT NULL,
            identifier_value TEXT NOT NULL,
            UNIQUE(entity_type, identifier_type, identifier_value)
        );

        CREATE TABLE IF NOT EXISTS MappingChanges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            from_identifier INTEGER NOT NULL REFERENCES Identifiers(id),
            to_identifier_previous INTEGER NOT NULL REFERENCES Identifiers(id),
            to_identifier_new INTEGER NOT NULL REFERENCES Identifiers(id),
            timestamp INTEGER NOT NULL
        );
    "#;

        let get_changes = r#"
        SELECT
            mc.id,
            mc.timestamp,
            f.entity_type,
            f.identifier_type,
            f.identifier_value,
            op.identifier_type as old_type__TEXT__NULLABLE,
            op.identifier_value as old_value__TEXT__NULLABLE,
            n.identifier_type as new_type__TEXT__NULLABLE,
            n.identifier_value as new_value__TEXT__NULLABLE
        FROM MappingChanges mc, Identifiers f, Identifiers op, Identifiers n
        WHERE mc.from_identifier = f.id
        AND mc.to_identifier_previous = op.id
        AND mc.to_identifier_new = n.id
        AND mc.timestamp > :since__INTEGER
        ORDER BY mc.timestamp ASC
    "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    let _ = migrate(&db).await?;
    let changes = get_changes(&db, 1).await?;
    println!("{:?}", changes);

    Ok(())
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
