use crate::errors::JsonPayloadError;
use dade::Model;
use ntex::http::{HttpMessage, Payload};
use ntex::util::{stream_recv, BytesMut};
use ntex::web::{ErrorRenderer, FromRequest, HttpRequest};
use std::future::Future;
use std::ops;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

type PinBox<T> = Pin<Box<T>>;

pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Deconstruct to an inner value
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T, Err: ErrorRenderer> FromRequest<Err> for Json<T>
where
    T: Model + 'static,
{
    type Error = JsonPayloadError;
    type Future = PinBox<dyn Future<Output = Result<Self, Self::Error>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let (limit, ctype) = req
            .app_state::<JsonConfig>()
            .map(|c| (c.limit, c.content_type.clone()))
            .unwrap_or((32768, None));

        let fut = JsonBody::new(req, payload, ctype).limit(limit);
        Box::pin(async move {
            match fut.await {
                Err(e) => Err(e),
                Ok(data) => Ok(Json(data)),
            }
        })
    }
}

#[derive(Clone)]
pub struct JsonConfig {
    limit: usize,
    content_type: Option<Arc<dyn Fn(mime::Mime) -> bool + Send + Sync>>,
}

impl JsonConfig {
    /// Change max size of payload. By default max size is 32Kb
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set predicate for allowed content types
    pub fn content_type<F>(mut self, predicate: F) -> Self
    where
        F: Fn(mime::Mime) -> bool + Send + Sync + 'static,
    {
        self.content_type = Some(Arc::new(predicate));
        self
    }
}

impl Default for JsonConfig {
    fn default() -> Self {
        JsonConfig {
            limit: 32768,
            content_type: None,
        }
    }
}

/// Request's payload json parser, it resolves to a deserialized `T` value.
///
/// Returns error:
///
/// * content type is not `application/json`
///   (unless specified in [`JsonConfig`](struct.JsonConfig.html))
/// * content length is greater than 256k
struct JsonBody<U, E> {
    limit: usize,
    length: Option<usize>,
    #[cfg(feature = "compress")]
    stream: Option<Decoder<Payload>>,
    #[cfg(not(feature = "compress"))]
    stream: Option<Payload>,
    err: Option<E>,
    fut: Option<PinBox<dyn Future<Output = Result<U, E>>>>,
}

impl<U> JsonBody<U, JsonPayloadError>
where
    U: Model + 'static,
{
    /// Create `JsonBody` for request.
    fn new(
        req: &HttpRequest,
        payload: &mut Payload,
        ctype: Option<Arc<dyn Fn(mime::Mime) -> bool + Send + Sync>>,
    ) -> Self {
        // check content-type
        let json = if let Ok(Some(mime)) = req.mime_type() {
            mime.subtype() == mime::JSON
                || mime.suffix() == Some(mime::JSON)
                || ctype.as_ref().map_or(false, |predicate| predicate(mime))
        } else {
            false
        };

        if !json {
            return JsonBody {
                limit: 262_144,
                length: None,
                stream: None,
                fut: None,
                err: Some(JsonPayloadError::ContentType),
            };
        }

        let len = req
            .headers()
            .get("content-length")
            .and_then(|l| l.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        #[cfg(feature = "compress")]
        let payload = Decoder::from_headers(payload.take(), req.headers());
        #[cfg(not(feature = "compress"))]
        let payload = payload.take();

        JsonBody {
            limit: 262_144,
            length: len,
            stream: Some(payload),
            fut: None,
            err: None,
        }
    }

    /// Change max size of payload. By default max size is 256Kb
    fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl<U> Future for JsonBody<U, JsonPayloadError>
where
    U: Model + 'static,
{
    type Output = Result<U, JsonPayloadError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(ref mut fut) = self.fut {
            return Pin::new(fut).poll(cx);
        }

        if let Some(err) = self.err.take() {
            return Poll::Ready(Err(err));
        }

        let limit = self.limit;
        if let Some(len) = self.length.take() {
            if len > limit {
                return Poll::Ready(Err(JsonPayloadError::Overflow));
            }
        }
        let mut stream = self.stream.take().unwrap();

        self.fut = Some(Box::pin(async move {
            let mut body = BytesMut::with_capacity(8192);

            while let Some(item) = stream_recv(&mut stream).await {
                let chunk = item?;
                if (body.len() + chunk.len()) > limit {
                    return Err(JsonPayloadError::Overflow);
                } else {
                    body.extend_from_slice(&chunk);
                }
            }

            match U::parse_bytes(&body) {
                Ok(u) => Ok(u),
                Err(e) => Err(JsonPayloadError::Deserialize(e)),
            }
        }));

        self.poll(cx)
    }
}
