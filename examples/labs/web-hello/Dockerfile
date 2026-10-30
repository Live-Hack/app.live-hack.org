# Minimal lab: a web page that shows the flag injected by the platform as $FLAG.
FROM busybox:1.36
RUN adduser -D -H app
USER app
EXPOSE 8080
CMD ["sh", "-c", "mkdir -p /tmp/www && printf '<h1>Hello lab</h1><p>flag: %s</p>' \"$FLAG\" > /tmp/www/index.html && exec httpd -f -p 8080 -h /tmp/www"]
