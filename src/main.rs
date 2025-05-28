use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, http::header, web};
use awc::Client;
use futures::{future::ok, stream::once};
use tokio::fs::{File, metadata, remove_file};
use tokio_util::{
    bytes,
    codec::{BytesCodec, FramedRead},
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/file/bot{token}/{file_path}", web::get().to(file_handler))
            .default_service(web::route().to(proxy_api))
    })
    .bind(("0.0.0.0", 3000))?
    .run()
    .await
}

async fn proxy_api(req: HttpRequest, payload: web::Payload) -> Result<HttpResponse, Error> {
    let uri = req.uri().to_string();
    let client = Client::default();
    let mut forward = client.request(
        req.method().clone(),
        format!("http://telegram-bot-api:8081{}", uri),
    );

    // copy all headers except Host
    for (h, v) in req.headers().iter() {
        if h != &header::HOST {
            forward = forward.insert_header((h.clone(), v.clone()));
        }
    }

    // send and await response
    let mut upstream = forward.send_stream(payload).await.unwrap();

    // build our response
    let mut client_resp = HttpResponse::build(upstream.status());
    for (h, v) in upstream.headers().iter() {
        if h != &header::TRANSFER_ENCODING {
            client_resp.insert_header((h.clone(), v.clone()));
        }
    }

    // stream the body back
    let body_stream = once(ok::<_, Error>(upstream.body().await?));
    Ok(client_resp.streaming(body_stream))
}

async fn file_handler(
    _req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, Error> {
    let (bot_token, file_path) = path.into_inner();
    if file_path.contains("..") {
        return Err(actix_web::error::ErrorBadRequest("invalid path"));
    }

    let base = Path::new("/var/lib/telegram-bot-api/file");
    let full = base.join(format!("bot{}/{}", bot_token, file_path));
    let meta = metadata(&full)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("no file"))?;
    let size = meta.len();
    let file = File::open(&full)
        .await
        .map_err(|_| actix_web::error::ErrorNotFound("cannot open"))?;
    let framed = FramedRead::new(file, BytesCodec::new());
    let counter = Arc::new(AtomicU64::new(0));
    let stream = CountingStream {
        inner: framed,
        read: counter.clone(),
        expected: size,
        path: full.clone(),
    };

    let mut resp = HttpResponse::Ok();
    resp.insert_header((
        header::CONTENT_TYPE,
        mime_guess::from_path(&full)
            .first_or_octet_stream()
            .to_string(),
    ));
    resp.insert_header((header::CONTENT_LENGTH, size.to_string()));
    Ok(resp.streaming(stream))
}

struct CountingStream {
    inner: FramedRead<File, BytesCodec>,
    read: Arc<AtomicU64>,
    expected: u64,
    path: PathBuf,
}

impl futures_core::Stream for CountingStream {
    type Item = Result<bytes::Bytes, Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match std::pin::Pin::new(&mut self.inner).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(chunk))) => {
                self.read.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                std::task::Poll::Ready(Some(Ok(chunk.into())))
            }
            x => x.map(|opt| {
                opt.map(|result| match result {
                    Ok(bytes) => Result::Ok(bytes.into()),
                    Err(error) => Result::Err(Error::from(error)),
                })
            }),
        }
    }
}

impl Drop for CountingStream {
    fn drop(&mut self) {
        if self.read.load(Ordering::Relaxed) == self.expected {
            let p = self.path.clone();
            tokio::spawn(async move {
                let _ = remove_file(p).await;
            });
        }
    }
}
