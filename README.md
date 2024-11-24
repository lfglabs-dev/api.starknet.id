## Description

StarknetID provides a robust interface for managing and querying StarkNet identities, making it easier to integrate identity services into applications built on StarkNet.

## Features

- Identity resolution and management
- Domain name services
- Secure API endpoints
- Docker containerization support
- Rust-powered performance

## Prerequisites

Before you begin, ensure you have the following installed:
- Rust (latest stable version) [Rust Installation Guide](https://doc.rust-lang.org/book/ch01-01-installation.html)
- Docker and Docker Compose [Docker Installation Guide](https://docs.docker.com/get-started/get-docker/)
- Git [Git Installation Guide](https://git-scm.com/downloads)

## Installation
1. Fork the repo

2. Clone the forked repository:
```bash
git clone git clone https://github.com/<your-user>/api.starknet.id.git
cd api.starknet.id
```

3. Create Branch
```bash
git checkout -b fix-[issue-number]
```

4. Build the project:
```bash
cargo build
```

5. Set up environment variables:
   - Copy the template configuration file:
   ```bash
   cp config.template.toml config.toml
   ```
   - Update the configuration with your specific settings

## Running the API

### Using Docker (Recommended)

1. Build and start the containers:
```bash
docker-compose up -d
```

### Using Cargo

1. Run in development mode:
```bash
cargo run
```

2. Run in production mode:
```bash
cargo run --release
```

## Configuration

The API can be configured using the `config.toml` file. Key configuration options include:
- Server settings
- Database connections
- StarkNet node configuration
- Logging preferences

## Troubleshooting

### Common Issues

1. **Docker Build Fails**
   - Ensure Docker is running
   - Check if required ports are available
   - Verify Docker has sufficient resources

2. **API Connection Issues**
   - Verify the configuration in config.toml
   - Check if the StarkNet node is accessible
   - Ensure all required environment variables are set

3. **Compilation Errors**
   - Update Rust to the latest stable version
   - Clear cargo cache and rebuild
   - Check for missing dependencies

## Contributing

We welcome contributions to the StarknetID API! To keep our workflow smooth, please follow these guidelines:

1. **Assignment**: Only create a pull request if you've been assigned to the issue.

2. **Timeframe**: Complete the task within 3 business days of assignment.

3. **Closing the Issue**: In your PR description, close the issue by writing `Close #[issue_id]`.

4. **Review Process**:
   - Once you've submitted your PR, change the label to "ready for review".
   - If changes are requested, address them and then update the label back to "ready for review" once done.

5. **Testing**: Test your PR locally before pushing, and verify that tests and build are working after pushing.

6. **Pull Request Steps**:
   - Fork the repository
   - Create a feature branch
   - Commit your changes
   - Push to your branch
   - Create a Pull Request


## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contact

For questions and support, please join our telegram channel https://t.me/SQcontributors