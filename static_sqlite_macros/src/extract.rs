use sqlparser::ast::{Expr, Query, Select, SelectItem, SetExpr};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

#[derive(Debug, PartialEq)]
enum Name {
    Table(Vec<Ident>),
    Column(Vec<Ident>),
    Alias(Vec<Ident>),
}

#[test]
fn test_extract_columns() {
    let sql = "SELECT users.id, name as user_name, email FROM users";
    let dialect = GenericDialect {};
    let ast = Parser::parse_sql(&dialect, sql).unwrap();

    let expected = vec![
        Name::Column(vec![Ident::new("users"), Ident::new("id")]),
        Name::Alias(vec![Ident::new("name"), Ident::new("user_name")]),
        Name::Column(vec![Ident::new("email")]),
    ];

    let result = extract_columns(&ast[0]);
    assert_eq!(result, expected);
}

fn extract_columns(stmt: &Statement) -> Vec<Name> {
    match stmt {
        Statement::Query(query) => {
            if let SetExpr::Select(select) = query.body.as_ref() {
                return select
                    .projection
                    .iter()
                    .filter_map(|item| match item {
                        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(ids)) => {
                            Some(Name::Column(ids.clone()))
                        }
                        SelectItem::ExprWithAlias { expr, alias } => match expr {
                            Expr::Identifier(id) => {
                                Some(Name::Alias(vec![id.clone(), alias.clone()]))
                            }
                            _ => None,
                        },
                        SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
                            Some(Name::Column(vec![id.clone()]))
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
        _ => vec![],
    }
}
