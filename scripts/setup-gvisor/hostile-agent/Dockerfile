# A deliberately hostile agent image for validating match isolation.
#
# On start it runs a battery of escape/abuse probes against the match network
# and host, prints a PASS/FAIL report (PASS = the probe was correctly blocked),
# then idles so the container can be inspected. It intentionally does NOT
# implement the real agent gRPC contract — this is a security smoke test run by
# hand (or by scripts/setup-gvisor/run-hostile-check.sh), not a match agent.
FROM alpine:latest
RUN apk add --no-cache iproute2 busybox-extras curl
COPY probe.sh /probe.sh
RUN chmod +x /probe.sh
ENTRYPOINT ["/probe.sh"]
