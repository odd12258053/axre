use axre;
use dade_derive::model;
use ntex::web;

#[model]
struct User {
    #[field(min_length = 1, max_length = 10)]
    name: String,
}

async fn index(payload: axre::types::Json<User>) -> String {
    format!("Welcome {}!", payload.name)
}

fn services(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/welcome").route(web::Route::new().method(ntex::http::Method::POST).to(index)),
    );
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
    web::server(|| web::App::new().configure(services))
        .bind("127.0.0.1:8080")?
        .run()
        .await
}

