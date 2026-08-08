build app:
    docker build . -f apps/{{app}}/Dockerfile -t {{app}}:latest
