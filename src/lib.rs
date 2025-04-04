extern crate self as static_sqlite;
pub use static_sqlite_async::{
    execute, execute_all, open, query, query_first, rows, stream, Error, FromRow, Result,
    Savepoint, Sqlite, Value,
};
pub use static_sqlite_core::FirstRow;
pub use static_sqlite_macros::sql;
