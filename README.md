# static_sqlite

An easy way to map sql to rust functions and structs

# Quickstart

```rust
use static_sqlite::{sql, Result, self};

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

#[tokio::main]
async fn main() -> Result<()> {
    let db = static_sqlite::open("db.sqlite3").await?;
    let _ = migrate(&db).await?;
    let users = insert_user(&db, "swlkr").await?;
    let user = users.first().unwrap();

    assert_eq!(user.id, 1);
    assert_eq!(user.name, "swlkr");

    Ok(())
}
```

# Use

```sh
cargo add --git https://github.com/swlkr/static_sqlite
```


# Example for First

With Sqlite you often do small queries that just return on row. For this a fn with the postfix _first
is automatically created.

```
    sql! {
        let migrate = r#"
            create table Row (
                id integer primary key autoincrement,
                txt text NOT NULL
            )
        "#;

        let insert_row = r#"
            insert into Row (txt) values (:txt) returning *
        "#;

        let select_row = r#"
            select * from Row where id = :id
        "#;
    }

    let db = static_sqlite::open(":memory:").await?;
    migrate(&db).await?;

    insert_row(&db, "test1").await?.first_row()?;
    insert_row(&db, "test2").await?.first_row()?;

    match select_row_first(&db, 1).await? {
        Some(row) => assert_eq!(row.txt, "test1"),
        None => panic!("Row 1 not found"),
    }
```

# Example for Streams

If you don't want to read the whole result set into memory, you can get the result
as a futures::Stream over items of the derived type. The fn with the postfix _stream is automatically
created.

```
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

```

# Example with aliased columns and type-hints

Sometimes the type of either a bound parameter or a returned column can not be inferred by
sqlite / static_sqlite (see [sqlite3 docs](https://www.sqlite.org/c3ref/column_decltype.html))

In this case you can use type-hints to help the static_sqlite to use the correct type.

To use type-hints your parameter or column name needs to follow the following format:

```
<name>__<INTEGER|REAL|TEXT|BLOB>
```

or

```
<name>__<INTEGER|REAL|TEXT|BLOB>__<NULLABLE|NOT_NULL>
```

If not explicitly specified, the parameter or column is assumed to be NOT NULL.

```rust
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
        select
            u1.name as friend1_name__TEXT,
            u2.name as friend2_name__TEXT
        from Friendship, User as u1, User as u2
        where Friendship.user_id = u1.id
              and Friendship.friend_id = u2.id
              and Friendship.id = :friendship_id__INTEGER
    "#;
}


#[tokio::main]
async fn main() -> Result<()> {
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
```



# Treesitter

```
((macro_invocation
   macro:
     [
       (scoped_identifier
         name: (_) @_macro_name)
       (identifier) @_macro_name
     ]
   (token_tree
     (identifier)
     (raw_string_literal
       (string_content) @injection.content)))
 (#eq? @_macro_name "sql")
 (#set! injection.language "sql")
 (#set! injection.include-children))
```

Happy hacking!
