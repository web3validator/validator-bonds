use std::collections::HashMap;

use tokio_postgres::{types::ToSql, Client};

pub struct UpdateQueryCombiner<'a> {
    pub updates: u64,
    statement: String,
    values_names: String,
    where_condition: String,
    params: Vec<&'a (dyn ToSql + Sync)>,
}

impl<'a> UpdateQueryCombiner<'a> {
    pub fn new(
        table_name: String,
        updates: String,
        values_names: String,
        where_condition: String,
    ) -> Self {
        Self {
            updates: 0,
            statement: format!("UPDATE {} SET {} FROM (VALUES", table_name, updates).to_string(),
            values_names,
            where_condition,
            params: vec![],
        }
    }

    pub fn add(&mut self, values: &mut Vec<&'a (dyn ToSql + Sync)>, types: HashMap<usize, String>) {
        let separator = if self.updates == 0 { " " } else { "," };
        let mut query_end = "(".to_string();
        for i in 0..values.len() {
            if i > 0 {
                query_end.push_str(",");
            }
            query_end.push_str(&format!("${}", i + 1 + self.params.len()));
            if let Some(t) = types.get(&i) {
                query_end.push_str(&format!("::{}", t));
            };
        }
        query_end.push_str(")");

        self.params.append(values);
        self.statement
            .push_str(&format!("{}{}", separator, query_end));
        self.updates += 1;
    }

    pub async fn execute(&mut self, client: &mut Client) -> anyhow::Result<Option<u64>> {
        if self.updates == 0 {
            return Ok(None);
        }

        self.statement.push_str(&format!(
            ") AS {} WHERE {}",
            self.values_names, self.where_condition
        ));

        Ok(Some(client.execute(&self.statement, &self.params).await?))
    }
}

pub struct InsertQueryCombiner<'a> {
    pub insertions: u64,
    statement: String,
    params: Vec<&'a (dyn ToSql + Sync)>,
}

pub fn to_fixed(a: f64, decimals: i32) -> u64 {
    (a * 10f64.powi(decimals)).round() as u64
}

pub fn to_fixed_for_sort(a: f64) -> u64 {
    to_fixed(a, 4)
}

impl<'a> InsertQueryCombiner<'a> {
    pub fn new(table_name: String, columns: String) -> Self {
        Self {
            insertions: 0,
            statement: format!("INSERT INTO {} ({}) VALUES", table_name, columns).to_string(),
            params: vec![],
        }
    }

    pub fn add(&mut self, values: &mut Vec<&'a (dyn ToSql + Sync)>) {
        let separator = if self.insertions == 0 { " " } else { "," };
        let mut query_end = "(".to_string();
        for i in 0..values.len() {
            if i > 0 {
                query_end.push_str(",");
            }
            query_end.push_str(&format!("${}", i + 1 + self.params.len()));
        }
        query_end.push_str(")");

        self.params.append(values);
        self.statement
            .push_str(&format!("{}{}", separator, query_end));
        self.insertions += 1;
    }

    pub async fn execute(&self, client: &mut Client) -> anyhow::Result<Option<u64>> {
        if self.insertions == 0 {
            return Ok(None);
        }

        // println!("{}", self.statement);
        // println!("{:?}", self.params);

        Ok(Some(client.execute(&self.statement, &self.params).await?))
    }
}
