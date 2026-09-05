package prism.agent;

import java.io.OutputStream;
import java.lang.instrument.Instrumentation;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.*;
import java.security.spec.AlgorithmParameterSpec;
import java.util.Base64;

/**
 * Lightweight, zero-dependency Java Agent for transparent RSA private key extraction.
 * Intercepts JVM KeyPairGenerator to push server private key to Prism middleware API.
 */
public class PrismAgent {
    private static String targetUrl = "http://localhost:8080/middlewares/minecraft/data";
    private static String authToken = "";
    private static int serverPort = 25565;
    private static String middlewareName = "minecraft";
    private static volatile boolean keyCaptured = false;

    public static void premain(String agentArgs, Instrumentation inst) {
        parseArgs(agentArgs);
        installProvider();
        System.out.println("[PrismAgent] Java Agent initialized. Monitoring RSA KeyPair generation for Prism middleware: " + middlewareName);
    }

    private static void parseArgs(String args) {
        String envUrl = System.getenv("PRISM_URL");
        if (envUrl != null && !envUrl.trim().isEmpty()) {
            targetUrl = envUrl.trim();
        }
        String envToken = System.getenv("PRISM_TOKEN");
        if (envToken != null) {
            authToken = envToken.trim();
        }
        String envPort = System.getenv("PRISM_PORT");
        if (envPort != null) {
            try {
                serverPort = Integer.parseInt(envPort.trim());
            } catch (Exception ignored) {}
        }
        String envMw = System.getenv("PRISM_MIDDLEWARE");
        if (envMw != null && !envMw.trim().isEmpty()) {
            middlewareName = envMw.trim();
        }

        if (args != null && !args.trim().isEmpty()) {
            String[] parts = args.split(",");
            for (String part : parts) {
                String[] kv = part.split("=", 2);
                if (kv.length == 2) {
                    String k = kv[0].trim().toLowerCase();
                    String v = kv[1].trim();
                    if (k.equals("url")) {
                        targetUrl = v;
                    } else if (k.equals("token")) {
                        authToken = v;
                    } else if (k.equals("port")) {
                        try {
                            serverPort = Integer.parseInt(v);
                        } catch (Exception ignored) {}
                    } else if (k.equals("middleware")) {
                        middlewareName = v;
                    }
                } else if (kv.length == 1 && !kv[0].trim().isEmpty()) {
                    targetUrl = kv[0].trim();
                }
            }
        }

        if (!targetUrl.contains("/middlewares/")) {
            if (targetUrl.endsWith("/")) {
                targetUrl = targetUrl + "middlewares/" + middlewareName + "/data";
            } else {
                targetUrl = targetUrl + "/middlewares/" + middlewareName + "/data";
            }
        }
    }

    private static void installProvider() {
        try {
            Provider interceptor = new PrismSecurityProvider();
            Security.insertProviderAt(interceptor, 1);
        } catch (Throwable t) {
            System.err.println("[PrismAgent] Warning: Failed to install PrismSecurityProvider: " + t.getMessage());
        }
    }

    public static void onKeyPairGenerated(KeyPair keyPair) {
        if (keyCaptured || keyPair == null || keyPair.getPrivate() == null) {
            return;
        }
        keyCaptured = true;
        byte[] privateKeyDer = keyPair.getPrivate().getEncoded();
        if (privateKeyDer == null || privateKeyDer.length == 0) {
            return;
        }

        final String base64Der = Base64.getEncoder().encodeToString(privateKeyDer);
        final String endpoint = targetUrl;
        final String token = authToken;
        final int port = serverPort;

        Thread pusher = new Thread(new Runnable() {
            @Override
            public void run() {
                String jsonPayload = String.format("{\"port\":%d,\"data\":\"%s\"}", port, base64Der);
                int attempts = 0;
                while (true) {
                    attempts++;
                    try {
                        URL url = new URL(endpoint);
                        HttpURLConnection conn = (HttpURLConnection) url.openConnection();
                        conn.setRequestMethod("POST");
                        conn.setRequestProperty("Content-Type", "application/json");
                        if (token != null && !token.isEmpty()) {
                            conn.setRequestProperty("Authorization", "Bearer " + token);
                        }
                        conn.setDoOutput(true);
                        conn.setConnectTimeout(3000);
                        conn.setReadTimeout(3000);

                        try (OutputStream os = conn.getOutputStream()) {
                            os.write(jsonPayload.getBytes(StandardCharsets.UTF_8));
                            os.flush();
                        }

                        int code = conn.getResponseCode();
                        if (code >= 200 && code < 300) {
                            System.out.println("[PrismAgent] Successfully dispatched RSA private key to Prism (" + endpoint + ")");
                            break;
                        } else {
                            System.err.println("[PrismAgent] Push failed with HTTP status " + code + ", retrying in 1s...");
                        }
                    } catch (Throwable ex) {
                        if (attempts <= 5 || attempts % 10 == 0) {
                            System.err.println("[PrismAgent] Push attempt " + attempts + " failed (" + ex.getMessage() + "), retrying in 1s...");
                        }
                    }

                    try {
                        Thread.sleep(1000);
                    } catch (InterruptedException ie) {
                        break;
                    }
                }
            }
        }, "PrismAgent-KeyPusher");
        pusher.setDaemon(true);
        pusher.start();
    }

    public static class PrismSecurityProvider extends Provider {
        public PrismSecurityProvider() {
            super("PrismSecurityProvider", 1.0, "Prism Agent RSA KeyPair interceptor");
            put("KeyPairGenerator.RSA", PrismRsaKeyPairGenerator.class.getName());
        }
    }

    public static class PrismRsaKeyPairGenerator extends KeyPairGeneratorSpi {
        private KeyPairGeneratorSpi delegate;

        public PrismRsaKeyPairGenerator() {
            try {
                Provider[] providers = Security.getProviders();
                for (Provider p : providers) {
                    if (p.getName().equals("PrismSecurityProvider")) continue;
                    Provider.Service svc = p.getService("KeyPairGenerator", "RSA");
                    if (svc != null) {
                        this.delegate = (KeyPairGeneratorSpi) svc.newInstance(null);
                        break;
                    }
                }
            } catch (Throwable ignored) {}
        }

        @Override
        public void initialize(int keysize, SecureRandom random) {
            if (delegate != null) {
                delegate.initialize(keysize, random);
            }
        }

        @Override
        public void initialize(AlgorithmParameterSpec params, SecureRandom random) throws InvalidAlgorithmParameterException {
            if (delegate != null) {
                delegate.initialize(params, random);
            }
        }

        @Override
        public KeyPair generateKeyPair() {
            KeyPair kp = null;
            if (delegate != null) {
                kp = delegate.generateKeyPair();
            }
            if (kp != null) {
                PrismAgent.onKeyPairGenerated(kp);
            }
            return kp;
        }
    }
}
