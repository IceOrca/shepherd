FROM node:24.20.0-alpine AS builder
WORKDIR /app/shepherd_client

COPY package*.json ./
RUN npm ci
