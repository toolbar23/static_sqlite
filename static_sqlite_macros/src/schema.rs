use proc_macro2::Span;
use sqlparser::ast::{
    AlterTableOperation, ColumnDef, ColumnOption, Expr, FunctionArg, FunctionArgExpr, Ident,
    ObjectName, ObjectType, Query, Select, SelectItem, SetExpr, Statement, TableAlias, TableFactor,
    TableWithJoins, Value,
};
use std::collections::HashMap;
use syn::{Error, Result};

use crate::SqlExpr;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub struct Alias<'a> {
    alias: &'a Ident,
    table: Table<'a>,
}

pub type AliasMap<'a> = HashMap<Table<'a>, Alias<'a>>;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub struct Table<'a> {
    pub name: &'a ObjectName,
    pub alias: &'a Option<TableAlias>,
}

pub fn table_aliased<'a>(name: &'a ObjectName, alias: &'a Option<TableAlias>) -> Table<'a> {
    Table { name, alias }
}

pub fn table<'a>(name: &'a ObjectName) -> Table {
    Table { name, alias: &None }
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Copy)]
pub struct Column<'a> {
    pub name: &'a sqlparser::ast::Ident,
    pub def: Option<&'a ColumnDef>,
    pub placeholder: Option<&'a str>,
    pub alias: Option<&'a Ident>,
}

pub fn column<'a>(name: &'a Ident) -> Column<'a> {
    Column {
        name,
        def: None,
        placeholder: None,
        alias: None,
    }
}

pub fn column_with_def<'a>(name: &'a Ident, def: Option<&'a ColumnDef>) -> Column<'a> {
    Column {
        name,
        def,
        placeholder: None,
        alias: None,
    }
}

#[derive(Debug)]
pub struct Schema<'a>(pub HashMap<Table<'a>, Vec<Column<'a>>>);

pub fn db_schema<'a>(migrate_expr: &'a SqlExpr) -> Result<Schema<'a>> {
    let mut set: HashMap<Table<'_>, Vec<Column<'_>>> = HashMap::new();

    let SqlExpr {
        ident, statements, ..
    } = migrate_expr;
    let span = ident.span();
    let _result = statements.iter().try_for_each(|stmt| match stmt {
        Statement::CreateTable { name, columns, .. } => {
            let columns = columns
                .iter()
                .map(|col| Column {
                    name: &col.name,
                    def: Some(col),
                    placeholder: None,
                    alias: None,
                })
                .collect();
            set.insert(table(name), columns);
            Ok(())
        }
        Statement::AlterTable {
            name, operations, ..
        } => {
            for op in operations {
                match op {
                    AlterTableOperation::AddColumn { column_def, .. } => {
                        match set.get_mut(&table(name)) {
                            Some(columns) => columns.push(Column {
                                name: &column_def.name,
                                def: Some(column_def),
                                placeholder: None,
                            }),
                            None => todo!(),
                        };
                    }
                    AlterTableOperation::DropColumn { column_name, .. } => {
                        match set.get_mut(&table(name)) {
                            Some(columns) => {
                                columns.retain(|col| col.name != column_name);
                            }
                            None => {}
                        }
                    }
                    AlterTableOperation::RenameColumn {
                        old_column_name,
                        new_column_name,
                    } => match set.get_mut(&table(name)) {
                        Some(columns) => {
                            match columns.iter().position(|col| col.name == old_column_name) {
                                Some(ix) => {
                                    let mut col = columns.remove(ix);
                                    col.name = new_column_name;
                                    columns.push(col);
                                }
                                None => {}
                            };
                        }
                        None => {}
                    },
                    AlterTableOperation::RenameTable { table_name } => {
                        match set.remove(&table(name)) {
                            Some(columns) => {
                                set.insert(table(table_name), columns);
                            }
                            None => {}
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        Statement::Drop {
            object_type, names, ..
        } => {
            match object_type {
                ObjectType::Table => match names.first() {
                    Some(name) => {
                        set.remove(&table(name));
                    }
                    None => {
                        return Err(Error::new(
                            span,
                            format!("drop statement {} requires a name", stmt.to_string()),
                        ))
                    }
                },
                _ => todo!(),
            }
            Ok(())
        }
        _ => Ok(()),
    })?;

    Ok(Schema(set))
}

pub fn query_schema<'a>(span: Span, statements: &'a Vec<Statement>) -> Result<Schema<'a>> {
    let mut set: HashMap<Table<'_>, Vec<Column<'_>>> = HashMap::new();

    statements.iter().try_for_each(|stmt| match stmt {
        Statement::Query(query) => {
            set_query_columns(query, span, &mut set)?;
            Ok(())
        }
        Statement::Insert {
            table_name,
            columns,
            returning,
            ..
        } => {
            let table = table(table_name);
            let mut columns: Vec<Column<'a>> = columns
                .iter()
                .map(|name| Column {
                    name,
                    def: None,
                    placeholder: Some("?"),
                })
                .collect();
            let returning_columns = returning_columns(span, returning)?;
            columns.extend(returning_columns);
            set.insert(table, columns);
            Ok(())
        }
        Statement::Update {
            table,
            assignments,
            from: _from,
            selection,
            returning,
        } => {
            let table = match &table.relation {
                TableFactor::Table { name, alias, .. } => table_aliased(name, alias),
                _ => todo!(),
            };
            let mut columns: Vec<_> = assignments
                .iter()
                .filter_map(|assign| {
                    let name = match assign.id.as_slice() {
                        [_schema, _table, column] => Some(column),
                        [_table, column] => Some(column),
                        [column] => Some(column),
                        _ => None,
                    };
                    match name {
                        Some(name) => match &assign.value {
                            Expr::Value(Value::Placeholder(val)) => Some(Column {
                                name,
                                def: None,
                                placeholder: Some(val.as_str()),
                            }),
                            _ => Some(Column {
                                name,
                                def: None,
                                placeholder: None,
                            }),
                        },
                        None => None,
                    }
                })
                .collect();
            let selection_columns = selection_columns(selection);
            columns.extend(selection_columns);
            let returning_columns = returning_columns(span, returning)?;
            columns.extend(returning_columns);
            set.insert(table, columns);
            Ok(())
        }
        Statement::Delete {
            from,
            selection,
            returning,
            ..
        } => {
            let table = match from.first() {
                None => {
                    return Err(syn::Error::new(
                        span,
                        "Delete statement requires at least one table",
                    ))
                }
                Some(table) => match &table.relation {
                    TableFactor::Table { name, alias, .. } => table_aliased(name, alias),
                    _ => todo!(),
                },
            };
            let mut columns = selection_columns(selection);
            let ret_columns = returning_columns(span, returning)?;
            columns.extend(ret_columns);
            set.insert(table, columns);
            Ok(())
        }
        Statement::CreateTable { name, columns, .. } => {
            for column in columns {
                let ColumnDef { options, .. } = column;
                options.iter().for_each(|opt| match &opt.option {
                    ColumnOption::ForeignKey {
                        foreign_table,
                        referred_columns,
                        ..
                    } => {
                        set.insert(
                            table(foreign_table),
                            referred_columns
                                .iter()
                                .map(|name| Column {
                                    name,
                                    def: None,
                                    placeholder: None,
                                })
                                .collect(),
                        );
                    }
                    _ => {}
                });
            }
            set.insert(
                table(name),
                columns
                    .iter()
                    .map(|col| Column {
                        name: &col.name,
                        def: Some(&col),
                        placeholder: None,
                    })
                    .collect(),
            );
            Ok(())
        }
        Statement::AlterTable { .. } => Ok(()),
        _ => todo!("fn query_schema Statement match statement"),
    })?;

    Ok(Schema(set))
}

pub fn alias_map<'a>(
    span: Span,
    statements: &'a Vec<Statement>,
) -> Result<Vec<(&'a Ident, &'a Ident)>> {
    let mut result = vec![];
    statements.iter().try_for_each(|stmt| {
        let aliases = match stmt {
            Statement::Query(query) => query_alias_map(span, &query)?,
            statement => todo!("fn alias_map {statement}"),
        };
        result.push(aliases);
        Ok::<(), Error>(())
    });
    let result = result.into_iter().flatten().collect::<Vec<_>>();
    Ok(result)
}

fn query_alias_map<'a>(span: Span, query: &'a Query) -> Result<Vec<(&'a Ident, &'a Ident)>> {
    let Query {
        with: _with,
        body,
        order_by: _order_by,
        ..
    } = query;
    match body.as_ref() {
        SetExpr::Select(select) => select_alias_map(span, select.as_ref()),
        SetExpr::Query(query) => query_alias_map(span, query),
        _ => todo!("fn set_query_columns"),
    }
}

fn select_alias_map<'a>(span: Span, select: &'a Select) -> Result<Vec<(&'a Ident, &'a Ident)>> {
    let select = select_alias_tuples(span, select);
    Ok(select)
}

fn select_alias_tuples<'a>(span: Span, select: &'a Select) -> Vec<(&'a Ident, &'a Ident)> {
    let Select {
        from, projection, ..
    } = select;

    let tables = from
        .iter()
        .map(|table_with_joins| {
            let from = relation_alias_tuples(&table_with_joins.relation);

            let mut joins = table_with_joins
                .joins
                .iter()
                .map(|join| relation_alias_tuples(&join.relation))
                .collect::<Vec<_>>();

            joins.insert(0, from);

            joins
        })
        .flat_map(|tup| tup)
        .filter_map(|tup| tup)
        .collect::<Vec<_>>();

    let columns = projection
        .iter()
        .map(|select_item| match select_item {
            SelectItem::UnnamedExpr(_) => todo!(),
            SelectItem::ExprWithAlias { expr, alias } => todo!(),
            SelectItem::QualifiedWildcard(_, _) => todo!(),
            SelectItem::Wildcard(_) => todo!(),
        })
        .collect::<Vec<_>>();

    tables
}

fn relation_alias_tuples<'a>(relation: &'a TableFactor) -> Option<(&'a Ident, &'a Ident)> {
    match relation {
        TableFactor::Table {
            name,
            alias,
            args,
            with_hints,
            version,
            partitions,
        } => match alias {
            Some(TableAlias {
                name: alias,
                columns,
            }) => match table_from_compound_ident(&name.0) {
                Some(ident) => Some((ident, alias)),
                None => todo!(),
            },
            None => None,
        },
        table_factor => todo!("fn select_containers table_factor {table_factor}"),
    }
}

fn set_query_columns<'a>(
    query: &'a Query,
    span: Span,
    set: &mut HashMap<Table<'a>, Vec<Column<'a>>>,
) -> Result<()> {
    let Query {
        with: _with,
        body,
        order_by,
        ..
    } = query;
    match body.as_ref() {
        SetExpr::Select(select) => {
            let tables = from_tables(&select.from);
            let table = match tables.first() {
                Some(table) => *table,
                None => return Err(Error::new(span, "Only one table in from supported for now")),
            };
            let mut columns = selection_columns(&select.selection);
            let projection_columns = select_items_columns(span, select.projection.as_slice())?;
            columns.extend(projection_columns);
            let order_by_columns: Vec<Column<'_>> = order_by
                .iter()
                .flat_map(|ob| expr_columns(&ob.expr))
                .collect();
            columns.extend(order_by_columns);
            set.insert(table, columns.into_iter().collect());
        }
        SetExpr::Query(query) => set_query_columns(query, span, set)?,
        _ => todo!("fn set_query_columns"),
    }
    Ok(())
}

fn from_tables(from: &Vec<TableWithJoins>) -> Vec<Table<'_>> {
    from.iter()
        .flat_map(|table| {
            let mut tables = match &table.relation {
                TableFactor::Table { name, alias, .. } => vec![table_aliased(name, alias)],
                _ => todo!("fn from_tables"),
            };
            let join_tables: Vec<_> = table
                .joins
                .iter()
                .map(|join| relation_table(&join.relation))
                .collect();
            tables.extend(join_tables);
            tables
        })
        .collect()
}

fn relation_table(relation: &TableFactor) -> Table<'_> {
    match relation {
        TableFactor::Table { name, alias, .. } => table_aliased(name, alias),
        _ => todo!("fn relation_table: other TableFactors"),
    }
}

fn returning_columns<'a>(
    span: Span,
    returning: &'a Option<Vec<SelectItem>>,
) -> Result<Vec<Column<'a>>> {
    match returning {
        Some(select_items) => select_items
            .iter()
            .filter_map(|si| match si {
                SelectItem::UnnamedExpr(expr) => match expr {
                    Expr::Identifier(ident) => Some(Ok(Column {
                        name: ident,
                        def: None,
                        placeholder: None,
                    })),
                    Expr::CompoundIdentifier(ident) => match compound_ident_column(ident) {
                        Some(col) => Some(Ok(col)),
                        None => None,
                    },
                    Expr::QualifiedWildcard(_) => Some(Err(Error::new(
                        span,
                        "RETURNING may not use \"{expr}.*\" wildcards",
                    ))),
                    _ => None,
                },
                SelectItem::ExprWithAlias { expr: _expr, alias } => Some(Ok(Column {
                    name: alias,
                    def: None,
                    placeholder: None,
                })),
                SelectItem::QualifiedWildcard(_, _) => Some(Err(Error::new(
                    span,
                    "RETURNING may not use \"{expr}.*\" wildcards",
                ))),
                SelectItem::Wildcard(_) => None,
            })
            .collect(),
        None => Ok(vec![]),
    }
}

fn select_items_columns<'a>(span: Span, select_items: &'a [SelectItem]) -> Result<Vec<Column<'a>>> {
    select_items
        .iter()
        .map(|si| match si {
            SelectItem::UnnamedExpr(expr) => match expr {
                Expr::Identifier(name) => Err(Error::new(
                    span,
                    format!("{name} is not allowed. Only qualified column names supported"),
                )),
                Expr::CompoundIdentifier(name) => compound_ident_column(name).ok_or(Error::new(
                    span,
                    format!("{expr} is not allowed. Only qualified column names supported"),
                )),
                _ => todo!("fn select_items_columns selectitem match"),
            },
            SelectItem::ExprWithAlias { expr, alias, .. } => match expr {
                Expr::Identifier(name) => Ok(Column {
                    name,
                    def: None,
                    alias: Some(alias),
                    placeholder: None,
                }),
                Expr::CompoundIdentifier(name) => compound_ident_column(name).ok_or(Error::new(
                    span,
                    format!("{expr} is not allowed. Only qualified column names supported",),
                )),
                expr => todo!("fn select_items_columns ExprWithAlias {expr}"),
            },
            si => todo!("fn select_items_columns {si}"),
        })
        .collect()
}

fn compound_ident_column<'a>(name: &'a Vec<Ident>) -> Option<Column<'a>> {
    let name = match name.as_slice() {
        [_schema, _table, name] => Some(name),
        [_table, name] => Some(name),
        [name] => Some(name),
        _ => None,
    };

    match name {
        Some(name) => Some(Column {
            name,
            def: None,
            placeholder: None,
        }),
        None => None,
    }
}

pub fn table_from_compound_ident<'a>(name: &'a Vec<Ident>) -> Option<&'a Ident> {
    match name.as_slice() {
        [_schema, table, _name] => Some(table),
        [table, _name] => Some(table),
        [table] => Some(table),
        _ => None,
    }
}

fn selection_columns<'a>(selection: &'a Option<Expr>) -> Vec<Column<'a>> {
    match selection {
        Some(expr) => expr_columns(expr),
        None => vec![],
    }
}

fn expr_columns<'a>(expr: &'a Expr) -> Vec<Column<'a>> {
    match expr {
        Expr::BinaryOp { left, op: _, right } => match (left.as_ref(), right.as_ref()) {
            (Expr::Identifier(name), Expr::Value(Value::Placeholder(val))) if val == "?" => {
                vec![Column {
                    name,
                    def: None,
                    placeholder: Some(val.as_str()),
                }]
            }
            (Expr::CompoundIdentifier(parts), Expr::Value(Value::Placeholder(val)))
                if val == "?" =>
            {
                match parts.as_slice() {
                    [_schema_name, _table_name, name] => {
                        vec![Column {
                            name,
                            def: None,
                            placeholder: Some(val.as_str()),
                        }]
                    }
                    [_table_name, name] => {
                        vec![Column {
                            name,
                            def: None,
                            placeholder: Some(val.as_str()),
                        }]
                    }
                    _ => unreachable!("one part compound identifier?!"),
                }
            }
            (left, right) => {
                let mut columns = expr_columns(left);
                columns.extend(expr_columns(right));
                columns
            }
        },
        Expr::CompoundIdentifier(parts) => {
            let name = match parts.as_slice() {
                [_schema, _table, name] => Some(name),
                [_table, name] => Some(name),
                [name] => Some(name),
                _ => None,
            };

            match name {
                Some(name) => vec![Column {
                    name,
                    def: None,
                    placeholder: None,
                }],
                None => vec![],
            }
        }
        Expr::Identifier(name) => vec![Column {
            name,
            def: None,
            placeholder: None,
        }],
        Expr::Nested(expr) => expr_columns(expr),
        Expr::Function(func) => func
            .args
            .iter()
            .flat_map(|arg| match arg {
                FunctionArg::Named { name: _name, arg } => match arg {
                    FunctionArgExpr::Expr(expr) => expr_columns(expr),
                    FunctionArgExpr::QualifiedWildcard(_object_name) => todo!(),
                    FunctionArgExpr::Wildcard => todo!(),
                },
                FunctionArg::Unnamed(function_arg_expr) => match function_arg_expr {
                    FunctionArgExpr::Expr(expr) => expr_columns(expr),
                    FunctionArgExpr::QualifiedWildcard(_object_name) => todo!(),
                    FunctionArgExpr::Wildcard => todo!(),
                },
            })
            .collect::<Vec<_>>(),
        Expr::Case {
            conditions,
            results,
            else_result,
            ..
        } => {
            let mut cols = conditions
                .iter()
                .flat_map(|expr| expr_columns(expr))
                .collect::<Vec<_>>();
            let results = results.iter().flat_map(|expr| expr_columns(expr));
            cols.extend(results);
            if let Some(else_expr) = else_result {
                let columns = expr_columns(else_expr.as_ref());
                cols.extend(columns);
            }
            cols
        }
        expr => todo!("expr_columns rest of the ops {expr}"),
    }
}

pub fn query_table_names(query: &Box<Query>) -> Vec<&ObjectName> {
    match query.body.as_ref() {
        SetExpr::Select(select) => select
            .from
            .iter()
            .map(|table| match &table.relation {
                TableFactor::Table { name, .. } => name,
                _ => todo!(),
            })
            .collect::<Vec<_>>(),
        SetExpr::Query(query) => query_table_names(query),
        _ => todo!("query_table_names"),
    }
}

pub fn placeholder_len(stmt: &Statement) -> usize {
    match stmt {
        Statement::Insert { source, .. } => match source {
            Some(query) => {
                let Query { body, .. } = query.as_ref();
                match body.as_ref() {
                    SetExpr::Values(values) => values
                        .rows
                        .iter()
                        .flat_map(|expr| expr)
                        .collect::<Vec<_>>()
                        .len(),
                    SetExpr::Select(select) => {
                        let Select { selection, .. } = select.as_ref();
                        match selection.as_ref() {
                            Some(expr) => expr_columns(expr)
                                .iter()
                                .filter(|col| col.placeholder.is_some())
                                .collect::<Vec<_>>()
                                .len(),
                            None => 0,
                        }
                    }
                    _ => todo!("fn placeholders"),
                }
            }
            None => 0,
        },
        Statement::Update {
            assignments,
            selection,
            ..
        } => {
            let len = assignments.len();
            match selection {
                Some(expr) => expr_columns(expr).len() + len,
                None => len,
            }
        }
        Statement::Delete { selection, .. } => match selection {
            Some(expr) => expr_columns(expr).len(),
            None => todo!(),
        },
        Statement::Query(query) => {
            let Query { body, .. } = query.as_ref();
            match body.as_ref() {
                SetExpr::Values(values) => values
                    .rows
                    .iter()
                    .flat_map(|expr| expr)
                    .collect::<Vec<_>>()
                    .len(),
                SetExpr::Select(select) => {
                    let Select { selection, .. } = select.as_ref();
                    match selection.as_ref() {
                        Some(expr) => expr_columns(expr)
                            .iter()
                            .filter(|col| col.placeholder.is_some())
                            .collect::<Vec<_>>()
                            .len(),
                        None => 0,
                    }
                }
                _ => todo!("fn placeholders"),
            }
        }
        _ => 0,
    }
}
