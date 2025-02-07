
pub struct MiddlewareChain {
    middlewares: Vec<Box<dyn Middleware>>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new()
        }
    }

    pub fn add<M: Middleware>(&mut self, middleware: M) {
        self.middlewares.push(Box::new(middleware));
    }

    pub async fn execute_request_chain(
        &self,
        mut request: Request<Body>,
    ) -> Result<Request<Body>, MiddlewareError> {
        for middleware in &self.middlewares {
            request = middleware.handle_request(request).await?;
        }
        Ok(request)
    }

    pub async fn execute_response_chain(
        &self,
        mut response: Response<Body>,
    ) -> Result<Response<Body>, MiddlewareError> {
        // 응답은 역순으로 처리
        for middleware in self.middlewares.iter().rev() {
            response = middleware.handle_response(response).await?;
        }
        Ok(response)
    }
}