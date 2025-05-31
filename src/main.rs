use std::{path::Path, time::Duration};

use actix_files::NamedFile;
use actix_web::{
    App, Error, HttpRequest, HttpResponse, HttpServer, Responder, dev::PeerAddr, error, middleware,
    web,
};
use awc::Client;
use url::Url;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let forward_url = Url::parse("http://telegram-bot-api:8081").unwrap();

    let listen_addr = "0.0.0.0";
    let listen_port = 3000;

    log::info!(
        "starting HTTP server at http://{}:{}",
        &listen_addr,
        &listen_port
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Client::default()))
            .app_data(web::Data::new(forward_url.clone()))
            .wrap(middleware::Logger::default())
            .service(
                web::resource("/file/bot{token}/{file_path:.*}").route(web::get().to(file_handler)),
            )
            .default_service(web::route().to(proxy_api))
    })
    .bind((listen_addr, listen_port))?
    .run()
    .await
}

async fn proxy_api(
    req: HttpRequest,
    payload: web::Payload,
    peer_addr: Option<PeerAddr>,
    url: web::Data<Url>,
    client: web::Data<Client>,
) -> Result<HttpResponse, Error> {
    let mut new_url = (**url).clone();
    new_url.set_path(req.uri().path());
    new_url.set_query(req.uri().query());

    let forwarded_req = client
        .request_from(new_url.as_str(), req.head())
        .timeout(Duration::MAX)
        .no_decompress();

    // TODO: This forwarded implementation is incomplete as it only handles the unofficial
    // X-Forwarded-For header but not the official Forwarded one.
    let forwarded_req = match peer_addr {
        Some(PeerAddr(addr)) => {
            forwarded_req.insert_header(("x-forwarded-for", addr.ip().to_string()))
        }
        None => forwarded_req,
    };

    let res = forwarded_req
        .send_stream(payload)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let mut client_resp = HttpResponse::build(res.status());
    // Remove `Connection` as per
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
    for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
        client_resp.insert_header((header_name.clone(), header_value.clone()));
    }

    Ok(client_resp.streaming(res))
}

async fn file_handler(
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (bot_token, file_path) = path.into_inner();
    if bot_token.contains("..") {
        return Err(actix_web::error::ErrorBadRequest("invalid path"));
    }
    if file_path.contains("..") {
        return Err(actix_web::error::ErrorBadRequest("invalid path"));
    }

    log::debug!("{}, {}", bot_token, file_path);

    // full file path
    let path = Path::new("/var/lib/telegram-bot-api")
        .join(bot_token)
        .join(file_path);

    match NamedFile::open_async(&path).await {
        Ok(named_file) => {
            let res = named_file.respond_to(&req);
            Ok(res)
        }
        Err(err) => Err(Error::from(err)),
    }
}
