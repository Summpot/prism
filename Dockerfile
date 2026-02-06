# syntax=docker/dockerfile:1

FROM golang:1.25-alpine AS build

WORKDIR /src

RUN apk add --no-cache ca-certificates

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
    GOOS="${TARGETOS:-$(go env GOOS)}" GOARCH="${TARGETARCH:-$(go env GOARCH)}" \
    go build -trimpath -ldflags="-s -w" -o /out/prism ./cmd/prism

FROM alpine:3.21

RUN apk add --no-cache ca-certificates \
    && addgroup -S prism \
    && adduser -S prism -G prism

COPY --from=build /out/prism /usr/local/bin/prism

# Prism auto-detects prism.toml > prism.yaml > prism.yml > prism.json from CWD.
WORKDIR /config

EXPOSE 25565 8080

USER prism:prism

ENTRYPOINT ["/usr/local/bin/prism"]
