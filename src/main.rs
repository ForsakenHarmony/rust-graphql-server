#![feature(plugin)]
#![feature(proc_macro)]
#![plugin(rocket_codegen)]

#[macro_use]
extern crate juniper;
extern crate juniper_rocket;
extern crate rocket;
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate r2d2;
extern crate r2d2_diesel;

mod schema;

use diesel::prelude::*;
use diesel::pg::PgConnection;
use r2d2::Pool;
use r2d2_diesel::ConnectionManager;
use dotenv::dotenv;
use std::env;

use rocket::response::content;
use rocket::State;

use juniper::{
    gql_object, meta::MetaType, to_camel_case, Arguments, ExecutionResult, Executor,
    GraphQLType, IntoResolvable, Registry, RootNode, Type, Value, GraphQLObject, FieldResult
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
#[table_name="posts"]
pub struct NewPost {
    pub title: String,
    pub body: String,
}

struct Context {
    pool: Pool<ConnectionManager<PgConnection>>,
}

struct Query;

#[gql_object]
impl Query<Context = Context> {
    #[graphql(description="Hello there!!")]
    fn hello(_: &Executor<Context>) -> String {
        "Hello World".to_string()
    }

    #[graphql(description="Echo your message")]
    fn echo(_: &Executor<Context>, msg: String) -> String {
        msg
    }

    fn get_posts(executor: &Executor<Context>) -> FieldResult<Vec<Post>> {
        use schema::posts::dsl::*;

        let connection = executor.context().pool.get()?;

        let res = posts.filter(published.eq(true))
            .limit(5)
            .load::<Post>(&*connection)?;

        Ok(res)
    }
}

struct Mutation;

#[gql_object]
impl Mutation<Context = Context> {
    fn create_post(executor: &Executor<Context>, new_post: NewPost) -> FieldResult<Post> {
        use schema::posts::dsl::*;

        let conn = executor.context().pool.get()?;

        let res = diesel::insert_into(posts)
            .values(new_post)
            .get_result(&*conn)?;

        Ok(res)
    }

    fn publish_post(executor: &Executor<Context>, id: i32) -> FieldResult<Post> {
        use schema::posts::dsl::{posts, published};

        let conn = executor.context().pool.get()?;

        let res = diesel::update(posts.find(id))
            .set(published.eq(true))
            .get_result(&*conn)?;

        Ok(res)
    }
}

fn publish_post(executor: &Executor<Context>, id: i32) -> FieldResult<Post> {
    use schema::posts::dsl::{posts, published};

    let conn = executor.context().pool.get()?;

    let res = diesel::update(posts.find(id))
        .set(published.eq(true))
        .get_result(&*conn)?;

    Ok(res)
}

type Schema = RootNode<'static, Query, Mutation>;

#[get("/")]
fn graphiql() -> content::Html<String> {
    juniper_rocket::graphiql_source("/graphql")
}

#[post("/graphql", data = "<request>")]
fn post_graphql_handler(
    context: State<Context>,
    request: juniper_rocket::GraphQLRequest,
    schema: State<Schema>,
) -> juniper_rocket::GraphQLResponse {
    request.execute(&schema, &context)
}

pub fn create_connection_pool() -> Pool<ConnectionManager<PgConnection>> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let manager = ConnectionManager::new(database_url.clone());

    Pool::builder().build(manager).expect(&format!("Failed to create connection pool to {}", database_url))
}

fn main() {
    rocket::ignite()
        .manage(Context {
            pool: create_connection_pool()
        })
        .manage(Schema::new(
            Query {},
            Mutation {},
        ))
        .mount(
            "/",
            routes![graphiql, post_graphql_handler],
        )
        .launch();
}
