use axum::{
    routing::{get, post},
    Router,
};

use crate::{db::DbClient, AppConfig};

use super::{api, AppState};

pub async fn start_server(app_config: AppConfig, db_client: DbClient) {
    let app_state = AppState {
        channel_name: app_config.db.tasks_channel_name.clone(),
        max_seconds_to_sleep: app_config.max_seconds_to_sleep,
        db_client,
    };

    // build our application with a single route
    let app = Router::new()
        .route("/task", post(api::create_task).get(api::list_tasks))
        .route(
            "/task/:task_id",
            get(api::get_task).delete(api::delete_task),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(app_state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", app_config.listen_port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
