FROM node:24-bookworm-slim AS builder
WORKDIR /app/shepherd_frontend

COPY package*.json ./
RUN npm ci
