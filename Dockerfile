# syntax=docker/dockerfile:1

FROM golang:1.25 AS build

WORKDIR /src

# Cache deps first
COPY go.mod go.sum ./
RUN go mod download

# Copy the rest and build
COPY . ./

ARG TARGETOS
ARG TARGETARCH

# Static binary is ideal for distroless/scratch.
ENV CGO_ENABLED=0

RUN --mount=type=cache,target=/root/.cache/go-build \
    GOOS=${TARGETOS:-linux} GOARCH=${TARGETARCH:-amd64} \
    go build -trimpath -ldflags="-s -w" -o /out/prism ./cmd/prism

FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=build /out/prism /usr/local/bin/prism

# Prism auto-detects prism.toml > prism.yaml > prism.yml > prism.json from CWD.
WORKDIR /config

EXPOSE 25565 8080

USER nonroot:nonroot

ENTRYPOINT ["/usr/local/bin/prism"]
