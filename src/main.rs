#![feature(proc_macro)]

#[macro_use]
extern crate juniper;
extern crate logger;
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate r2d2;
extern crate r2d2_diesel;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate hyper;
extern crate futures;
extern crate serde_json;

mod schema;
mod http;

use diesel::prelude::*;
use diesel::pg::PgConnection;
use r2d2::{Pool, PooledConnection, Error};
use r2d2_diesel::ConnectionManager;
use dotenv::dotenv;
use std::env;
use hyper::{Request,Body};
use juniper::{
  gql_object, ExecutionResult, Executor, Type, GraphQLObject, FieldResult, RootNode
};

use schema::posts;

#[derive(Queryable, GraphQLObject)]
pub struct Post {
  pub id: i32,
  pub title: String,
  pub body: String,
  pub published: bool,
}

#[derive(Insertable, GraphQLInputObject)]
#[table_name = "posts"]
pub struct NewPost {
  pub title: String,
  pub body: String,
}

struct Context {
  pool: Pool<ConnectionManager<PgConnection>>,
}

impl Context {
  fn db(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, Error> {
    self.pool.get()
  }
}

struct Query;

#[gql_object]
impl Query<Context=Context> {
  #[graphql(description = "Hello there!!")]
  fn hello(_: &Executor<Context>) -> String {
    "Hello World".to_string()
  }

  #[graphql(description = "Echo your message")]
  fn echo(_: &Executor<Context>, msg: String) -> String {
    msg
  }

  fn get_posts(executor: &Executor<Context>) -> FieldResult<Vec<Post>> {
    use schema::posts::dsl::*;

    let connection = executor.context().db()?;

    let res = posts.filter(published.eq(true))
        .limit(5)
        .load::<Post>(&*connection)?;

    Ok(res)
  }
}

struct Mutation;

#[gql_object]
impl Mutation<Context=Context> {
  fn create_post(executor: &Executor<Context>, new_post: NewPost) -> FieldResult<Post> {
    use schema::posts::dsl::*;

    let conn = executor.context().db()?;

    let res = diesel::insert_into(posts)
        .values(new_post)
        .get_result(&*conn)?;

    Ok(res)
  }

  fn publish_post(executor: &Executor<Context>, id: i32) -> FieldResult<Post> {
    use schema::posts::dsl::{posts, published};

    let conn = executor.context().db()?;

    let res = diesel::update(posts.find(id))
        .set(published.eq(true))
        .get_result(&*conn)?;

    Ok(res)
  }
}

type Schema<'a> = RootNode<'a, Query, Mutation>;

pub fn create_connection_pool(database_url: String) -> Pool<ConnectionManager<PgConnection>> {
  let manager = ConnectionManager::new(database_url.clone());

  Pool::builder().build(manager).expect(&format!("Failed to create connection pool to {}", database_url))
}

fn main() {
  dotenv().ok();
  pretty_env_logger::init();

  let database_url = env::var("DATABASE_URL")
      .expect("DATABASE_URL must be set");

  let pool = create_connection_pool(database_url);

  let context_factory = move |_: &Request<Body>| {
    Context { pool: pool.clone() }
  };

  let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
  let host = env::var("LISTEN").unwrap_or_else(|_| format!("0.0.0.0:{}", port).to_string());

  http::Apollo::new(Schema::new(Query {}, Mutation {}), context_factory).start(Some(&host));
}
