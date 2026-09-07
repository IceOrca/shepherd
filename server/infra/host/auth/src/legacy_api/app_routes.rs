use axum::Router;

/// Finalized application routers grouped by host security policy.
///
/// Applications attach their own state before returning this value. The host
/// then applies its shared public, authenticated, and tenant-owner layers.
pub struct AppRoutes {
    pub public: Router,
    pub protected: Router,
    pub admin: Router,
}

impl AppRoutes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn public(mut self, router: Router) -> Self {
        self.public = self.public.merge(router);
        self
    }

    pub fn protected(mut self, router: Router) -> Self {
        self.protected = self.protected.merge(router);
        self
    }

    pub fn admin(mut self, router: Router) -> Self {
        self.admin = self.admin.merge(router);
        self
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.public = self.public.merge(other.public);
        self.protected = self.protected.merge(other.protected);
        self.admin = self.admin.merge(other.admin);
        self
    }
}

impl Default for AppRoutes {
    fn default() -> Self {
        Self {
            public: Router::new(),
            protected: Router::new(),
            admin: Router::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, routing::get};

    use super::AppRoutes;

    #[test]
    fn route_groups_can_be_built_and_merged() {
        let first: AppRoutes = AppRoutes::new().public(Router::new().route("/health", get(|| async {})));
        let second: AppRoutes = AppRoutes::new()
            .protected(Router::new().route("/profile", get(|| async {})))
            .admin(Router::new().route("/settings", get(|| async {})));

        let _routes: AppRoutes = first.merge(second);
    }
}
