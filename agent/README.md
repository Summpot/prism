# Prism Java Agent (`prism-agent`)

Decoupled, zero-dependency Java Agent for transparent RSA private key extraction and injection into Prism's middleware API (`POST /middlewares/minecraft/data`).

## Features

- **Zero Third-Party Dependencies**: Pure standard JCA (`KeyPairGeneratorSpi`, `Provider`). No ASM, ByteBuddy, or external JARs required.
- **Cross-Version Compatibility**: Compiles with the host's existing JDK (Java 8, 11, 17, 21+) to prevent JVM bytecode mismatch errors.
- **Non-blocking Asynchronous Dispatch**: Intercepts the RSA `KeyPair` at generation time and dispatches via background daemon thread with automatic retry without stalling server boot.
- **Prism Zero-Java Rule**: Prism's core remains a 100% pure Rust binary without requiring JRE/JDK on the proxy host.

## Compilation

Run the build script in your server environment:

- **Linux / macOS**:
  ```bash
  sh agent/build.sh
  ```
- **Windows**:
  ```cmd
  agent\build.bat
  ```

This generates `prism-agent.jar` in the `agent/` directory.

## Usage

Add `-javaagent` to your Minecraft server startup command:

```bash
java -javaagent:/path/to/prism-agent.jar -jar server.jar nogui
```

### Configuration Options

Options can be configured via agent arguments or environment variables:

1. **Via JVM Agent Argument**:

   ```bash
   -javaagent:prism-agent.jar=url=http://prism:8080,token=cluster-secret,port=25565
   ```

2. **Via Environment Variables**:
   ```bash
   export PRISM_URL="http://prism:8080"
   export PRISM_TOKEN="cluster-secret"
   export PRISM_PORT="25565"
   export PRISM_MIDDLEWARE="minecraft"
   java -javaagent:prism-agent.jar -jar server.jar nogui
   ```
