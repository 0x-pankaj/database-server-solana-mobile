use poem::{get, handler, http::StatusCode, listener::TcpListener, post, web::{Data, Json, Path}, EndpointExt, Route, Server};
use serde::{Deserialize, Serialize};
use sqlx::{pool::PoolOptions, FromRow, Pool, Postgres};
use validator::ValidateEmail;
// Removed anyhow::{Ok, Result}; not needed for this file

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Load database URL from environment or use default
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/postgres".to_string());

    // Initialize database connection pool
    let pool = PoolOptions::<Postgres>::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Define API routes
    let app = Route::new()
        .at("/email/:email", get(check_email_handler))
        .at("/users", post(create_user_handler))
        .data(pool);

    // Start server
    Server::new(TcpListener::bind("0.0.0.0:8003"))
        .run(app)
        .await?;

    Ok(())
}

// User model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
struct User {
    id: uuid::Uuid,
    email: String,
    active: bool,
    private_key: String,
    aggregated_public_key: String,
}

// Request struct for creating a user
#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    private_key: String,
    aggregated_public_key: String,
}

// Response struct for email check
#[derive(Serialize)]
struct EmailResponse {
    exists: bool,
    aggregated_public_key: Option<String>
}

// Check if an email exists in the users table
#[handler]
async fn check_email_handler(
    Path(email): Path<String>,
    Data(pool): Data<&Pool<Postgres>>,
) -> poem::Result<impl poem::IntoResponse> {
    if !email.validate_email() {
        return Err(poem::Error::from_status(StatusCode::BAD_REQUEST));
    }

    // Query for the user's aggregated_public_key by email
    let row = sqlx::query!(
        "SELECT aggregated_public_key FROM users WHERE email = $1",
        email
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| poem::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR))?;

    let (exists, aggregated_public_key) = match row {
        Some(record) => (true, Some(record.aggregated_public_key)),
        None => (false, None),
    };

    Ok(Json(EmailResponse { exists, aggregated_public_key }))
}


#[derive(Serialize)]
struct CreateUserResponse {
    success: bool,
}

#[handler]
// Create a new user
async fn create_user_handler(
    Data(pool): Data<&Pool<Postgres>>,
    Json(req): Json<CreateUserRequest>,
) -> poem::Result<impl poem::IntoResponse> {
    println!("hitted");
    println!("{:?}{:?}{:?}", req.aggregated_public_key,req.email, req.private_key);
    
    // if !req.email.validate_email() || req.private_key.len() < 32 || req.aggregated_public_key.len() < 32 {
    //     return Err(poem::Error::from_status(StatusCode::BAD_REQUEST));
    // }

    // println!("Request: {:?}", req.aggregated_public_key, );

    // Begin a transaction directly on the pool reference
    let mut tx = pool.begin().await
        .map_err(|_| poem::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR))?;

    let user = sqlx::query_as!(
        User,
        r#"
        INSERT INTO users (email, private_key, aggregated_public_key)
        VALUES ($1, $2, $3)
        RETURNING id, email, active, private_key, aggregated_public_key
        "#,
        req.email,
        req.private_key,
        req.aggregated_public_key
    )
    .fetch_one(&mut *tx) // Fix 4: Use &mut *tx to get the correct reference
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.constraint() == Some("users_email_key") {
                return poem::Error::from_status(StatusCode::CONFLICT);
            }
        }
        poem::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    tx.commit().await
        .map_err(|_| poem::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR))?;

    // Ok(Json(user))
    Ok(Json(CreateUserResponse{
        success: user.active
    }))




}
