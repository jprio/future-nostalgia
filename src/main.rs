use hyper::{server::conn::AddrStream, service::Service, Body, Request, Response, Server};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{
    convert::Infallible,
    future::{ready, Ready},
    task::{Context, Poll},
};
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::time::Sleep;
use tokio_util::sync::PollSemaphore;

#[tokio::main]
async fn main() {
    Server::bind(&([127, 0, 0, 1], 1025).into())
        .serve(MyServiceFactory::default())
        .await
        .unwrap();
}

struct MyServiceFactory {
    num_connected: Arc<AtomicU64>,
    semaphore: PollSemaphore,
    permit: Option<OwnedSemaphorePermit>,
}

impl Default for MyServiceFactory {
    fn default() -> Self {
        Self {
            num_connected: Default::default(),
            semaphore: PollSemaphore::new(Arc::new(Semaphore::new(5))),
            permit: None,
        }
    }
}

impl Service<&AddrStream> for MyServiceFactory {
    type Response = MyService;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.permit.is_none() {
            self.permit = Some(futures::ready!(self.semaphore.poll_acquire(_cx)).unwrap());
        }
        Ok(()).into()
    }

    fn call(&mut self, req: &AddrStream) -> Self::Future {
        let permit = self.permit.take().expect(
            "you didn't drive me to readiness did you? you know that's a tower crime right?",
        );

        let prev = self.num_connected.fetch_add(1, Ordering::SeqCst);
        println!(
            "⬆️ {} connections (accepted {})",
            prev + 1,
            req.remote_addr()
        );
        ready(Ok(MyService {
            num_connected: self.num_connected.clone(),
            permit,
        }))
    }
}
pin_project_lite::pin_project! {
    struct PretendFuture {
        #[pin]
        sleep: Sleep,
        response: Option<Response<Body>>,
    }
}

impl Future for PretendFuture {
    type Output = Result<Response<Body>, Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        futures::ready!(this.sleep.poll(cx));
        Ok(this.response.take().unwrap()).into()
    }
}

struct MyService {
    num_connected: Arc<AtomicU64>,
    permit: OwnedSemaphorePermit,
}

impl Drop for MyService {
    fn drop(&mut self) {
        let prev = self.num_connected.fetch_sub(1, Ordering::SeqCst);
        println!("⬇️ {} connections (dropped)", prev - 1);
    }
}
impl Service<Request<Body>> for MyService {
    type Response = Response<Body>;
    type Error = Infallible;
    //type Future = Ready<Result<Self::Response, Self::Error>>;
    type Future = PretendFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        println!("{} {}", req.method(), req.uri());
        PretendFuture {
            sleep: tokio::time::sleep(Duration::from_millis(250)),
            response: Some(Response::builder().body("Hello World!\n".into()).unwrap()),
        }
    }
}
