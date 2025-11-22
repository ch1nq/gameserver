FROM registry:2.8

# Save original entrypoint
# Copy custom entrypoint script that wraps the original
COPY apps/registry/entrypoint.sh /custom-entrypoint.sh
RUN chmod +x /custom-entrypoint.sh

# Use our custom entrypoint
ENTRYPOINT ["/custom-entrypoint.sh"]
CMD ["serve", "/etc/docker/registry/config.yml"]